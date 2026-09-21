mod config;
mod download;
mod engine;
mod mcp;
mod state;
mod tunnel;

use config::{Config, MCP_PORT};
use engine::Status;
use state::{AppState, Shared};
use std::time::{Duration, Instant};
use tauri::{Manager, State};

// ---------------------------------------------------------------- engine control

async fn do_start(st: Shared) {
    {
        let mut e = st.engine.lock().await;
        if e.has_child() {
            return;
        }
        e.status = Status::Loading;
        e.load_started = Some(Instant::now());
        e.last_oom = None;
    }
    // Whatever the desktop is using right now is the gauge's "other" segment.
    if let Some((used, _)) = engine::vram() {
        *st.vram_baseline.lock().unwrap() = used;
        *st.vram_peak.lock().unwrap() = used;
    }
    st.emit().await;

    let cfg = st.cfg.lock().await.clone();
    match engine::spawn(&cfg).await {
        Ok(child) => st.engine.lock().await.set_child(child),
        Err(e) => {
            let mut g = st.engine.lock().await;
            g.status = Status::Error {
                message: e.to_string(),
            };
            g.load_started = None;
            drop(g);
            st.emit().await;
            return;
        }
    }

    let t0 = Instant::now();
    match engine::wait_ready(Duration::from_secs(180)).await {
        Ok(()) => {
            let secs = t0.elapsed().as_secs();
            let mut g = st.engine.lock().await;
            g.status = Status::Ready;
            g.loaded_tier = Some(cfg.tier.clone());
            g.load_started = None;
            g.load_secs = Some(secs);
            drop(g);
            let mut c = st.cfg.lock().await;
            c.last_load_secs = Some(secs);
            c.save(&st.cfg_path);
        }
        Err(e) => {
            if let Some(mut c) = st.engine.lock().await.take_child() {
                engine::kill(&mut c).await;
            }
            let mut g = st.engine.lock().await;
            g.status = Status::Error {
                message: e.to_string(),
            };
            g.load_started = None;
        }
    }
    st.emit().await;
}

async fn do_stop(st: Shared) {
    {
        let mut e = st.engine.lock().await;
        if !e.has_child() {
            return;
        }
        e.status = Status::Stopping;
    }
    st.emit().await;
    if let Some(mut c) = st.engine.lock().await.take_child() {
        engine::kill(&mut c).await;
    }
    let mut e = st.engine.lock().await;
    e.status = Status::Off;
    e.loaded_tier = None;
    e.busy = None;
    drop(e);
    *st.vram_peak.lock().unwrap() = 0;
    st.emit().await;
}

// ---------------------------------------------------------------- commands

#[tauri::command]
async fn get_state(st: State<'_, Shared>) -> Result<state::Snapshot, String> {
    Ok(st.snapshot().await)
}

#[tauri::command]
async fn start_engine(st: State<'_, Shared>) -> Result<(), String> {
    let s = st.inner().clone();
    tauri::async_runtime::spawn(do_start(s));
    Ok(())
}

#[tauri::command]
async fn stop_engine(st: State<'_, Shared>) -> Result<(), String> {
    do_stop(st.inner().clone()).await;
    Ok(())
}

/// Selecting a tier is intent, not an action: the panel shows the mismatch and the
/// Stop button becomes "Restart with <tier>". Only a restart actually swaps weights.
#[tauri::command]
async fn select_tier(st: State<'_, Shared>, id: String) -> Result<(), String> {
    {
        let mut c = st.cfg.lock().await;
        c.tier = id;
        c.save(&st.cfg_path);
    }
    st.emit().await;
    Ok(())
}

#[tauri::command]
async fn restart_engine(st: State<'_, Shared>) -> Result<(), String> {
    let s = st.inner().clone();
    tauri::async_runtime::spawn(async move {
        do_stop(s.clone()).await;
        do_start(s).await;
    });
    Ok(())
}

#[tauri::command]
async fn set_tunnel(st: State<'_, Shared>, on: bool) -> Result<(), String> {
    {
        let mut c = st.cfg.lock().await;
        c.tunnel_enabled = on;
        c.save(&st.cfg_path);
    }
    let s = st.inner().clone();
    if on {
        tauri::async_runtime::spawn(tunnel::start(s));
    } else {
        tunnel::stop(&s).await;
    }
    Ok(())
}

#[tauri::command]
async fn mark_copied(st: State<'_, Shared>) -> Result<(), String> {
    st.tunnel.lock().await.is_new = false;
    st.emit().await;
    Ok(())
}

#[tauri::command]
async fn start_install(st: State<'_, Shared>) -> Result<(), String> {
    tauri::async_runtime::spawn(download::install(st.inner().clone()));
    Ok(())
}

#[tauri::command]
async fn pause_install(st: State<'_, Shared>, paused: bool) -> Result<(), String> {
    *st.paused.lock().unwrap() = paused;
    if !paused {
        tauri::async_runtime::spawn(download::install(st.inner().clone()));
    }
    st.emit().await;
    Ok(())
}

#[tauri::command]
async fn retry_download(st: State<'_, Shared>, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn(download::retry(st.inner().clone(), id));
    Ok(())
}

#[tauri::command]
async fn test_generate(st: State<'_, Shared>, prompt: String) -> Result<(), String> {
    let s = st.inner().clone();
    tauri::async_runtime::spawn(async move {
        s.set_busy(Some("Generating 1024×1024".into())).await;
        let out = engine::generate(engine::GenRequest {
            prompt: prompt.clone(),
            negative: String::new(),
            width: 1024,
            height: 1024,
            steps: 20,
            seed: -1,
            refs: vec![],
        })
        .await;
        s.set_busy(None).await;
        match out {
            Ok(png) => {
                let dir = s.cfg.lock().await.outputs_dir();
                std::fs::create_dir_all(&dir).ok();
                let path =
                    dir.join(format!("{}.png", chrono::Local::now().format("%Y%m%d-%H%M%S")));
                if std::fs::write(&path, &png).is_ok() {
                    s.push_gallery(state::GalleryItem {
                        path: path.to_string_lossy().to_string(),
                        prompt,
                        width: 1024,
                        height: 1024,
                        at: chrono::Local::now().format("%H:%M").to_string(),
                    });
                }
            }
            Err(e) => s.engine.lock().await.last_oom = Some(e.to_string()),
        }
        s.emit().await;
    });
    Ok(())
}

#[tauri::command]
async fn reveal(st: State<'_, Shared>, path: Option<String>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let target = match path {
        Some(p) => p,
        None => st
            .cfg
            .lock()
            .await
            .outputs_dir()
            .to_string_lossy()
            .to_string(),
    };
    let h = st.handle.get().ok_or("no app handle")?;
    h.opener()
        .open_path(target, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Diagnostics channel for the webview: a silent render failure looks like a frozen app.
#[tauri::command]
fn ui_log(msg: String) {
    eprintln!("[ui] {msg}");
}

#[tauri::command]
async fn show_log(st: State<'_, Shared>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let p = st.cfg.lock().await.log_path().to_string_lossy().to_string();
    let h = st.handle.get().ok_or("no app handle")?;
    h.opener().open_path(p, None::<&str>).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- MCP over HTTP

async fn serve_mcp(st: Shared) {
    use axum::extract::Request;
    use axum::middleware::Next;
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    };

    let secret = st.secret().await;
    let svc = StreamableHttpService::new(
        {
            let st = st.clone();
            move || Ok(mcp::Qwen::new(st.clone()))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );

    // Two jobs, both keyed on the Host header.
    //
    // rmcp blocks non-loopback Host values to stop DNS rebinding, which would reject
    // everything arriving through the tunnel. Rewriting the header to loopback — but
    // only when it matches the hostname our own cloudflared minted — keeps that
    // protection for every other caller. Reaching this point already means the request
    // carried the secret path, since anything else is a 404 before the service runs.
    //
    // Seeing that hostname at all is also proof the URL pasted into claude.ai works,
    // which is what the panel reports back.
    async fn note_host(
        axum::extract::State(st): axum::extract::State<Shared>,
        mut req: Request,
        next: Next,
    ) -> axum::response::Response {
        let host = req
            .headers()
            .get("host")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());

        if let Some(host) = host {
            let mut t = st.tunnel.lock().await;
            let ours = t.host().as_deref() == Some(host.as_str());
            let first = ours && t.connected_at.is_none();
            if first {
                t.connected_at = Some(chrono::Local::now().format("%H:%M").to_string());
            }
            drop(t);

            if ours {
                if let Ok(v) = format!("127.0.0.1:{MCP_PORT}").parse() {
                    req.headers_mut().insert("host", v);
                }
            }
            if first {
                st.emit().await;
            }
        }
        next.run(req).await
    }

    let app = axum::Router::new()
        .nest_service(&format!("/{secret}/mcp"), svc)
        .layer(axum::middleware::from_fn_with_state(st.clone(), note_host));

    if let Ok(l) = tokio::net::TcpListener::bind(("0.0.0.0", MCP_PORT)).await {
        let _ = axum::serve(l, app).await;
    } else {
        let mut e = st.engine.lock().await;
        e.status = Status::Error {
            message: format!("Port {MCP_PORT} is in use by another program."),
        };
    }
}

// ---------------------------------------------------------------- entry

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let cfg_path = dir.join("config.json");
            let default_root = std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            let cfg = Config::load(&cfg_path, default_root);
            cfg.save(&cfg_path);

            // The gallery renders files straight off disk, so the outputs folder has to
            // be reachable through the asset protocol. It moves with the install root,
            // which a static scope in tauri.conf.json cannot express.
            std::fs::create_dir_all(cfg.outputs_dir()).ok();
            app.asset_protocol_scope()
                .allow_directory(cfg.outputs_dir(), false)
                .ok();

            let st: Shared = std::sync::Arc::new(AppState::new(cfg.clone(), cfg_path));
            let _ = st.handle.set(app.handle().clone());
            app.manage(st.clone());

            tauri::async_runtime::spawn(serve_mcp(st.clone()));

            if cfg.tunnel_enabled {
                tauri::async_runtime::spawn(tunnel::start(st.clone()));
            }
            // Resume an interrupted install without asking; the moving bar says it.
            if !st.missing_files(&cfg).is_empty() {
                tauri::async_runtime::spawn(download::install(st.clone()));
            }

            // One snapshot a second drives the VRAM gauge and every live counter.
            let tick = st.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    tick.emit().await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            start_engine,
            stop_engine,
            restart_engine,
            select_tier,
            set_tunnel,
            mark_copied,
            start_install,
            pause_install,
            retry_download,
            test_generate,
            reveal,
            show_log,
            ui_log
        ])
        .run(tauri::generate_context!())
        .expect("error while running Qwen Image Studio");
}
