use crate::config::{tier, Config, ENGINE_PORT};
use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use serde_json::json;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Status {
    Off,
    Loading,
    Ready,
    Stopping,
    Error { message: String },
}

#[derive(Default)]
pub struct Engine {
    child: Option<Child>,
    pub status: Status,
    pub loaded_tier: Option<String>,
    pub load_started: Option<Instant>,
    pub load_secs: Option<u64>,
    /// Set when a generation fails for lack of VRAM; cleared on the next success.
    pub last_oom: Option<String>,
    pub busy: Option<String>,
    pub busy_since: Option<Instant>,
    /// Seconds this job is expected to take, from the measured 1024² baseline.
    pub busy_eta: Option<u64>,
}

impl Default for Status {
    fn default() -> Self {
        Status::Off
    }
}


fn base() -> String {
    format!("http://127.0.0.1:{ENGINE_PORT}")
}

/// The one launch configuration proven to work on a 16 GB card; see docs/DESIGN.md
/// and the "Проверено на железе" section of the plan. Do not add `--offload-to-cpu`:
/// its staging allocation fails on CUDA 13 drivers even with VRAM free, and putting the
/// text encoder on the GPU leaves the VAE 1 MB short when editing.
fn args(cfg: &Config) -> Vec<String> {
    let m = cfg.models_dir();
    let t = tier(&cfg.tier);
    vec![
        "--diffusion-model".into(),
        m.join(t.file).to_string_lossy().into(),
        "--llm".into(),
        m.join("qwen3vl_8b_heretic-Q4_K_M.gguf")
            .to_string_lossy()
            .into(),
        "--llm_vision".into(),
        m.join("mmproj-qwen3vl_8b_heretic-f16.gguf")
            .to_string_lossy()
            .into(),
        "--vae".into(),
        m.join("qwen_image_2.1_vae_bf16.safetensors")
            .to_string_lossy()
            .into(),
        "--params-backend".into(),
        "te=cpu,diffusion=cuda0,vae=cuda0".into(),
        "--backend".into(),
        "te=cpu".into(),
        // SageAttention measured 56.1 s against flash attention's 65.1 s at 1024² / 20
        // steps, with the two images visually indistinguishable at the same seed.
        "--sage-attn".into(),
        "--vae-tiling".into(),
        // Without this sd.cpp reads the weights lazily, on the first generation: "Ready"
        // lights up before anything has been read and the whole disk cost lands on the
        // first image (measured: 130 s off a hard drive). Loading them up front puts that
        // wait in the Loading state, where the counter already is.
        "--eager-load".into(),
        "--cfg-scale".into(),
        "6.0".into(),
        "--listen-ip".into(),
        "127.0.0.1".into(),
        "--listen-port".into(),
        ENGINE_PORT.to_string(),
        "-v".into(),
    ]
}

pub async fn spawn(cfg: &Config) -> Result<Child> {
    let exe = cfg.sd_server();
    if !exe.exists() {
        bail!("Couldn't start: engine binary is missing. Reinstall.");
    }
    std::fs::create_dir_all(cfg.outputs_dir()).ok();
    let log = std::fs::File::create(cfg.log_path())?;
    let err = log.try_clone()?;

    let mut cmd = Command::new(&exe);
    cmd.args(args(cfg))
        .current_dir(cfg.bin_dir())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err))
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    Ok(cmd.spawn()?)
}

/// Polls `/sdcpp/v1/capabilities` until the weights are in. ~15 s in practice.
pub async fn wait_ready(timeout: Duration) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/sdcpp/v1/capabilities", base());
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(r) = client
            .get(&url)
            .timeout(Duration::from_secs(3))
            .send()
            .await
        {
            if r.status().is_success() {
                return Ok(());
            }
        }
        if Instant::now() > deadline {
            bail!("Engine did not come up in time. Show log");
        }
        tokio::time::sleep(Duration::from_millis(700)).await;
    }
}

pub struct GenRequest {
    pub prompt: String,
    pub negative: String,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub seed: i64,
    pub refs: Vec<String>,
}

/// Submits a job and polls it to completion. Returns the PNG bytes.
pub async fn generate(req: GenRequest) -> Result<Vec<u8>> {
    let client = reqwest::Client::new();
    let mut body = json!({
        "prompt": req.prompt,
        "negative_prompt": req.negative,
        "width": req.width,
        "height": req.height,
        "seed": req.seed,
        "batch_count": 1,
        "sample_params": {
            "sample_method": "euler",
            "sample_steps": req.steps,
            "guidance": { "txt_cfg": 6.0 }
        },
        "output_format": "png"
    });
    if !req.refs.is_empty() {
        body["ref_images"] = json!(req.refs);
        body["increase_ref_index"] = json!(true);
    }

    let job: serde_json::Value = client
        .post(format!("{}/sdcpp/v1/img_gen", base()))
        .json(&body)
        .send()
        .await?
        .json()
        .await?;
    let poll = job["poll_url"]
        .as_str()
        .ok_or_else(|| anyhow!("engine did not accept the job"))?
        .to_string();

    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let s: serde_json::Value = client
            .get(format!("{}{}", base(), poll))
            .send()
            .await?
            .json()
            .await?;
        match s["status"].as_str().unwrap_or("") {
            "completed" => {
                let b64 = s["result"]["images"][0]["b64_json"]
                    .as_str()
                    .ok_or_else(|| anyhow!("engine returned no image"))?;
                use base64::Engine as _;
                return Ok(base64::engine::general_purpose::STANDARD.decode(b64)?);
            }
            "failed" | "cancelled" => {
                let m = s["error"]["message"].as_str().unwrap_or("generation failed");
                bail!(friendly(m));
            }
            _ => {}
        }
    }
}

/// sd.cpp reports every VRAM shortfall as "returned no results"; say something useful.
fn friendly(raw: &str) -> String {
    if raw.contains("no results") || raw.to_lowercase().contains("memory") {
        "Out of memory. Try a smaller size, or switch to Q4_K_M.".into()
    } else {
        raw.into()
    }
}

/// Measured with SageAttention on an RTX 4070: 1024² at 20 steps takes 56 s, and the
/// cost tracks pixel count almost linearly. Good enough to put a number beside the
/// elapsed seconds instead of inventing a progress bar.
pub fn eta_secs(w: u32, h: u32, steps: u32) -> u64 {
    let px = (w as f64 * h as f64) / (1024.0 * 1024.0);
    (px * 56.0 * (steps as f64 / 20.0)).round().max(5.0) as u64
}

/// (used MiB, total MiB). Returns None when nvidia-smi is unavailable.
pub fn vram() -> Option<(u64, u64)> {
    let mut cmd = std::process::Command::new("nvidia-smi");
    cmd.args([
        "--query-gpu=memory.used,memory.total",
        "--format=csv,noheader,nounits",
    ]);
    // Polled once a second for the gauge. Without this the console window it opens
    // flashes on screen every second.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let line = s.lines().next()?;
    let mut it = line.split(',').map(|p| p.trim().parse::<u64>().ok());
    Some((it.next()??, it.next()??))
}

pub async fn kill(child: &mut Child) {
    let _ = child.kill().await;
    // sd-server holds the port for a moment after exit.
    tokio::time::sleep(Duration::from_millis(400)).await;
}

impl Engine {
    pub fn take_child(&mut self) -> Option<Child> {
        self.child.take()
    }
    pub fn set_child(&mut self, c: Child) {
        self.child = Some(c);
    }
    pub fn has_child(&self) -> bool {
        self.child.is_some()
    }
}
