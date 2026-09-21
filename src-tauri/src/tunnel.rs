use crate::config::MCP_PORT;
use crate::state::Shared;
use serde::Serialize;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TunnelState {
    Off,
    Connecting,
    Up,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Tunnel {
    pub state: TunnelState,
    pub url: Option<String>,
    /// True once a URL is minted and until the user copies it. Drives the blue Copy button.
    pub is_new: bool,
    /// Set when a request actually arrives on the tunnel hostname — proof the paste worked.
    pub connected_at: Option<String>,
    pub message: Option<String>,
}

impl Default for Tunnel {
    fn default() -> Self {
        Self {
            state: TunnelState::Off,
            url: None,
            is_new: false,
            connected_at: None,
            message: None,
        }
    }
}

impl Tunnel {
    /// The hostname alone, for matching the Host header of incoming requests.
    pub fn host(&self) -> Option<String> {
        self.url
            .as_ref()
            .and_then(|u| u.strip_prefix("https://"))
            .map(|r| r.split('/').next().unwrap_or(r).to_string())
    }
}

/// A fresh `winget install` puts cloudflared on the system PATH, but every process
/// already running keeps the old environment — including this one. Check the default
/// install locations before giving up.
fn cloudflared_path() -> std::path::PathBuf {
    for p in [
        r"C:\Program Files (x86)\cloudflared\cloudflared.exe",
        r"C:\Program Files\cloudflared\cloudflared.exe",
    ] {
        let p = std::path::PathBuf::from(p);
        if p.exists() {
            return p;
        }
    }
    std::path::PathBuf::from("cloudflared")
}

fn find_url(line: &str) -> Option<String> {
    let i = line.find("https://")?;
    let rest = &line[i..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '|' || c == '"')
        .unwrap_or(rest.len());
    let url = rest[..end].trim_end_matches('/');
    url.ends_with(".trycloudflare.com").then(|| url.to_string())
}

pub async fn stop(state: &Shared) {
    if let Some(mut c) = state.tunnel_child.lock().await.take() {
        let _ = c.kill().await;
    }
    let mut t = state.tunnel.lock().await;
    *t = Tunnel::default();
    drop(t);
    state.emit().await;
}

pub async fn start(state: Shared) {
    {
        let mut t = state.tunnel.lock().await;
        t.state = TunnelState::Connecting;
        t.url = None;
        t.connected_at = None;
        t.message = None;
    }
    state.emit().await;

    let mut cmd = Command::new(cloudflared_path());
    cmd.args([
        "tunnel",
        "--url",
        &format!("http://127.0.0.1:{MCP_PORT}"),
        "--no-autoupdate",
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .stdin(Stdio::null());
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => {
            let mut t = state.tunnel.lock().await;
            t.state = TunnelState::Failed;
            t.message = Some(
                "cloudflared isn't installed. Run: winget install --id Cloudflare.cloudflared"
                    .into(),
            );
            drop(t);
            state.emit().await;
            return;
        }
    };

    // cloudflared prints the quick-tunnel URL on stderr, but that has moved between
    // releases, so both streams are watched.
    let err = child.stderr.take();
    let out = child.stdout.take();
    *state.tunnel_child.lock().await = Some(child);

    if let Some(e) = err {
        watch(state.clone(), e);
    }
    if let Some(o) = out {
        watch(state.clone(), o);
    }
}

fn watch<R>(state: Shared, reader: R)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let low = line.to_lowercase();
            if low.contains("err") || low.contains("failed") {
                eprintln!("[cloudflared] {line}");
            }
            let Some(origin) = find_url(&line) else {
                continue;
            };
            let full = format!("{origin}/{}/mcp", state.secret().await);
            let mut t = state.tunnel.lock().await;
            if t.url.as_deref() == Some(full.as_str()) {
                continue;
            }
            eprintln!("[tunnel] {full}");
            t.url = Some(full);
            t.state = TunnelState::Up;
            t.is_new = true;
            drop(t);
            state.emit().await;
        }
    });
}
