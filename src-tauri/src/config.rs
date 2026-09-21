use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MCP_PORT: u16 = 8765;
pub const ENGINE_PORT: u16 = 7860;

/// A downloadable DiT quantisation. Order here is display order, left to right.
pub struct Tier {
    pub id: &'static str,
    pub label: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub size: u64,
    pub desc: &'static str,
    /// False when the tier cannot actually run on a 16 GB card — see docs/DESIGN.md.
    pub fits_16gb: bool,
}

pub const TIERS: &[Tier] = &[
    Tier {
        id: "Q6_K",
        label: "Q6_K",
        file: "qwen_image_2.1-Q6_K.gguf",
        url: "https://huggingface.co/leejet/Qwen-Image-2.1-GGUF/resolve/main/qwen_image_2.1-Q6_K.gguf",
        size: 5_996_851_232,
        desc: "Best that fits 16 GB.",
        fits_16gb: true,
    },
    Tier {
        id: "Q4_K",
        label: "Q4_K_M",
        file: "qwen_image_2.1-Q4_K.gguf",
        url: "https://huggingface.co/leejet/Qwen-Image-2.1-GGUF/resolve/main/qwen_image_2.1-Q4_K.gguf",
        size: 4_197_494_816,
        desc: "Smallest. Visibly lower quality.",
        fits_16gb: true,
    },
    Tier {
        id: "Q8_0",
        label: "Q8_0",
        file: "qwen_image_2.1-Q8_0.gguf",
        url: "https://huggingface.co/leejet/Qwen-Image-2.1-GGUF/resolve/main/qwen_image_2.1-Q8_0.gguf",
        size: 7_687_155_744,
        desc: "Needs more than 16 GB at 1024².",
        fits_16gb: false,
    },
];

pub const DEFAULT_TIER: &str = "Q6_K";

pub fn tier(id: &str) -> &'static Tier {
    TIERS
        .iter()
        .find(|t| t.id == id)
        .unwrap_or_else(|| TIERS.iter().find(|t| t.id == DEFAULT_TIER).unwrap())
}

/// Everything that is not the DiT. `unzip` items land in `bin/`, the rest in `models/`.
pub struct Asset {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub file: &'static str,
    pub size: u64,
    pub unzip: bool,
}

pub const ASSETS: &[Asset] = &[
    Asset {
        id: "text_encoder",
        name: "Text encoder",
        url: "https://huggingface.co/pottokao/Qwen-Image-2.1-Text-Encoder-Heretic-GGUF/resolve/main/qwen3vl_8b_heretic-Q4_K_M.gguf",
        file: "qwen3vl_8b_heretic-Q4_K_M.gguf",
        size: 5_027_785_376,
        unzip: false,
    },
    Asset {
        id: "vision",
        name: "Vision projector",
        url: "https://huggingface.co/pottokao/Qwen-Image-2.1-Text-Encoder-Heretic-GGUF/resolve/main/mmproj-qwen3vl_8b_heretic-f16.gguf",
        file: "mmproj-qwen3vl_8b_heretic-f16.gguf",
        size: 1_159_030_464,
        unzip: false,
    },
    Asset {
        id: "vae",
        name: "VAE",
        url: "https://huggingface.co/Comfy-Org/Qwen-Image-2.1/resolve/main/vae/qwen_image_2.1_vae_bf16.safetensors",
        file: "qwen_image_2.1_vae_bf16.safetensors",
        size: 675_509_688,
        unzip: false,
    },
    Asset {
        id: "engine",
        name: "Engine",
        url: "https://github.com/leejet/stable-diffusion.cpp/releases/download/master-890-74988b2/sd-master-74988b2-bin-win-cuda12-x64.zip",
        file: "sd-win-cuda12.zip",
        size: 333_039_461,
        unzip: true,
    },
    Asset {
        id: "cudart",
        name: "CUDA runtime",
        url: "https://github.com/leejet/stable-diffusion.cpp/releases/download/master-890-74988b2/cudart-sd-bin-win-cu12-x64.zip",
        file: "cudart.zip",
        size: 563_452_046,
        unzip: true,
    },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub root: PathBuf,
    pub secret: String,
    pub tier: String,
    pub tunnel_enabled: bool,
    /// Seconds the last successful engine load took; drives the "loads in about N s" subline.
    pub last_load_secs: Option<u64>,
}

impl Config {
    pub fn models_dir(&self) -> PathBuf {
        self.root.join("models")
    }
    pub fn bin_dir(&self) -> PathBuf {
        self.root.join("bin")
    }
    pub fn outputs_dir(&self) -> PathBuf {
        self.root.join("outputs")
    }
    pub fn sd_server(&self) -> PathBuf {
        self.bin_dir().join("sd-server.exe")
    }
    pub fn log_path(&self) -> PathBuf {
        self.outputs_dir().join("sd-server.log")
    }

    pub fn load(path: &Path, default_root: PathBuf) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| Config {
                root: default_root,
                secret: random_secret(),
                tier: DEFAULT_TIER.to_string(),
                tunnel_enabled: false,
                last_load_secs: None,
            })
    }

    pub fn save(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, s);
        }
    }
}

pub fn random_secret() -> String {
    use rand::Rng;
    let mut b = [0u8; 16];
    rand::rng().fill(&mut b);
    hex::encode(b)
}
