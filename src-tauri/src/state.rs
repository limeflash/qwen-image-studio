use crate::config::{Config, Tier, TIERS};
use crate::engine::{Engine, Status};
use crate::tunnel::Tunnel;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize)]
pub struct GalleryItem {
    pub path: String,
    pub prompt: String,
    pub width: u32,
    pub height: u32,
    pub at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DlState {
    Pending,
    Connecting,
    Downloading,
    Stalled,
    Verifying,
    Unpacking,
    Done,
    Paused,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct DlRow {
    pub id: String,
    pub name: String,
    pub got: u64,
    pub total: u64,
    pub state: DlState,
    pub note: String,
}

#[derive(Serialize)]
pub struct TierView {
    pub id: &'static str,
    pub label: &'static str,
    pub size: u64,
    pub desc: &'static str,
    pub fits: bool,
    pub on_disk: bool,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub screen: &'static str,
    pub status: Status,
    pub loaded_tier: Option<String>,
    pub selected_tier: String,
    pub busy: Option<String>,
    pub busy_elapsed: Option<u64>,
    pub busy_eta: Option<u64>,
    pub load_elapsed: Option<u64>,
    pub last_load_secs: Option<u64>,
    pub last_oom: Option<String>,
    pub vram_used: u64,
    pub vram_total: u64,
    pub vram_engine: u64,
    pub vram_peak: u64,
    pub tiers: Vec<TierView>,
    pub local_url: String,
    pub tunnel: Tunnel,
    pub downloads: Vec<DlRow>,
    pub gallery: Vec<GalleryItem>,
    pub root: String,
    /// Free space on the volume holding `root`, and what the install needs.
    pub free_bytes: u64,
    pub needed_bytes: u64,
}

pub struct AppState {
    pub cfg: Mutex<Config>,
    pub cfg_path: PathBuf,
    pub engine: Mutex<Engine>,
    pub tunnel: Mutex<Tunnel>,
    pub tunnel_child: Mutex<Option<tokio::process::Child>>,
    pub downloads: StdMutex<Vec<DlRow>>,
    pub gallery: StdMutex<Vec<GalleryItem>>,
    /// VRAM used by the desktop before the engine started; lets the gauge split
    /// "other" from "engine" without asking the driver for per-process figures.
    pub vram_baseline: StdMutex<u64>,
    pub vram_peak: StdMutex<u64>,
    pub installing: StdMutex<bool>,
    pub paused: StdMutex<bool>,
    pub handle: OnceLock<AppHandle>,
}

pub type Shared = Arc<AppState>;

impl AppState {
    pub fn new(cfg: Config, cfg_path: PathBuf) -> Self {
        Self {
            cfg: Mutex::new(cfg),
            cfg_path,
            engine: Mutex::new(Engine::default()),
            tunnel: Mutex::new(Tunnel::default()),
            tunnel_child: Mutex::new(None),
            downloads: StdMutex::new(Vec::new()),
            gallery: StdMutex::new(Vec::new()),
            vram_baseline: StdMutex::new(0),
            vram_peak: StdMutex::new(0),
            installing: StdMutex::new(false),
            paused: StdMutex::new(false),
            handle: OnceLock::new(),
        }
    }

    pub async fn secret(&self) -> String {
        self.cfg.lock().await.secret.clone()
    }

    pub fn push_gallery(&self, item: GalleryItem) {
        let mut g = self.gallery.lock().unwrap();
        g.insert(0, item);
        g.truncate(8);
    }

    pub async fn set_busy(&self, what: Option<String>, eta: Option<u64>) {
        let mut e = self.engine.lock().await;
        e.busy_since = what.is_some().then(std::time::Instant::now);
        e.busy_eta = eta;
        e.busy = what;
        drop(e);
        self.emit().await;
    }

    pub fn missing_files(&self, cfg: &Config) -> Vec<&'static str> {
        let m = cfg.models_dir();
        let mut out = Vec::new();
        for a in crate::config::ASSETS {
            let present = if a.unzip {
                // The zips are deleted after unpacking; the binary is the real marker.
                cfg.sd_server().exists()
            } else {
                m.join(a.file).metadata().map(|x| x.len()).ok() == Some(a.size)
            };
            if !present {
                out.push(a.id);
            }
        }
        let t = crate::config::tier(&cfg.tier);
        if m.join(t.file).metadata().map(|x| x.len()).ok() != Some(t.size) {
            out.push(t.id);
        }
        out
    }

    /// True when some file is on disk but incomplete — a download that was interrupted
    /// rather than one that was never started. Only the former resumes by itself.
    pub fn install_in_progress(&self, cfg: &Config) -> bool {
        let m = cfg.models_dir();
        let partial = |p: std::path::PathBuf, want: u64| {
            matches!(p.metadata().map(|x| x.len()), Ok(n) if n > 0 && n < want)
        };
        crate::config::ASSETS.iter().any(|a| {
            let dir = if a.unzip { cfg.bin_dir() } else { m.clone() };
            partial(dir.join(a.file), a.size)
        }) || {
            // Only the selected tier is ever fetched, so only its leftovers count.
            let t = crate::config::tier(&cfg.tier);
            partial(m.join(t.file), t.size)
        }
    }

    fn tier_views(&self, cfg: &Config) -> Vec<TierView> {
        let m = cfg.models_dir();
        TIERS
            .iter()
            .map(|t: &Tier| TierView {
                id: t.id,
                label: t.label,
                size: t.size,
                desc: t.desc,
                fits: t.fits_16gb,
                on_disk: m.join(t.file).metadata().map(|x| x.len()).ok() == Some(t.size),
            })
            .collect()
    }

    pub async fn snapshot(&self) -> Snapshot {
        let cfg = self.cfg.lock().await.clone();
        let eng = self.engine.lock().await;
        let tunnel = self.tunnel.lock().await.clone();
        let downloads = self.downloads.lock().unwrap().clone();
        let gallery = self.gallery.lock().unwrap().clone();

        let (used, total) = crate::engine::vram().unwrap_or((0, 16384));
        let mut b = self.vram_baseline.lock().unwrap();
        if *b == 0 {
            *b = used;
        }
        let baseline = *b;
        drop(b);
        let mut peak = self.vram_peak.lock().unwrap();
        if used > *peak {
            *peak = used;
        }

        let installing = *self.installing.lock().unwrap();
        let needs_install = !self.missing_files(&cfg).is_empty();
        let screen = if installing || needs_install {
            "install"
        } else {
            "panel"
        };

        Snapshot {
            screen,
            status: eng.status.clone(),
            loaded_tier: eng.loaded_tier.clone(),
            selected_tier: cfg.tier.clone(),
            busy: eng.busy.clone(),
            busy_elapsed: eng.busy_since.map(|t| t.elapsed().as_secs()),
            busy_eta: eng.busy_eta,
            load_elapsed: eng.load_started.map(|t| t.elapsed().as_secs()),
            last_load_secs: cfg.last_load_secs,
            last_oom: eng.last_oom.clone(),
            vram_used: used,
            vram_total: total,
            vram_engine: used.saturating_sub(baseline),
            vram_peak: *peak,
            tiers: self.tier_views(&cfg),
            local_url: format!(
                "http://localhost:{}/{}/mcp",
                crate::config::MCP_PORT,
                cfg.secret
            ),
            tunnel,
            downloads,
            gallery,
            root: cfg.root.to_string_lossy().to_string(),
            free_bytes: crate::config::free_bytes(&cfg.root).unwrap_or(0),
            needed_bytes: crate::config::install_size(&cfg.tier),
        }
    }

    pub async fn emit(&self) {
        if let Some(h) = self.handle.get() {
            let snap = self.snapshot().await;
            if let Err(e) = h.emit("state", &snap) {
                eprintln!("emit(state) failed: {e}");
            }
            let title = match (&snap.status, snap.screen) {
                (_, "install") => {
                    let done: u64 = snap.downloads.iter().map(|d| d.got).sum();
                    let all: u64 = snap.downloads.iter().map(|d| d.total).sum();
                    if all > 0 && done < all {
                        format!("Installing {}% — Qwen Image Studio", done * 100 / all)
                    } else {
                        "Install — Qwen Image Studio".into()
                    }
                }
                (Status::Off, _) => "Off — Qwen Image Studio".into(),
                (Status::Loading, _) => "Loading — Qwen Image Studio".into(),
                (Status::Ready, _) => "Ready — Qwen Image Studio".into(),
                (Status::Stopping, _) => "Off — Qwen Image Studio".into(),
                (Status::Error { .. }, _) => "Error — Qwen Image Studio".into(),
            };
            if let Some(w) = h.get_webview_window("main") {
                let _ = w.set_title(&title);
            }
        }
    }
}
