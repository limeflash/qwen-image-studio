use crate::engine::{self, GenRequest, Status};
use crate::state::{GalleryItem, Shared};
use base64::Engine as _;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
    ErrorData as McpError, ServerHandler,
};
use std::path::PathBuf;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GenerateArgs {
    /// What to draw. English prompts follow the model's training most closely.
    pub prompt: String,
    /// Width in pixels. Rounded to a multiple of 32. Default 1024.
    #[serde(default)]
    pub width: Option<u32>,
    /// Height in pixels. Rounded to a multiple of 32. Default 1024.
    #[serde(default)]
    pub height: Option<u32>,
    /// Sampling steps. Default 20; above ~30 rarely pays for itself.
    #[serde(default)]
    pub steps: Option<u32>,
    /// Seed. Omit or pass -1 for a random one.
    #[serde(default)]
    pub seed: Option<i64>,
    /// What to avoid.
    #[serde(default)]
    pub negative_prompt: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EditArgs {
    /// The change to make, e.g. "change the sign text to 'OPEN'".
    pub prompt: String,
    /// Up to 10 reference images: absolute file paths, or raw base64 PNG/JPEG.
    pub images: Vec<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub steps: Option<u32>,
    #[serde(default)]
    pub seed: Option<i64>,
}

#[derive(Clone)]
pub struct Qwen {
    pub state: Shared,
    tool_router: ToolRouter<Qwen>,
}

fn snap(v: u32) -> u32 {
    (v.clamp(256, 2048) / 32) * 32
}

fn err(msg: impl Into<String>) -> McpError {
    McpError::internal_error(msg.into(), None)
}

impl Qwen {
    pub fn new(state: Shared) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    async fn require_ready(&self) -> Result<(), McpError> {
        let st = self.state.engine.lock().await.status.clone();
        match st {
            Status::Ready => Ok(()),
            Status::Loading => Err(err("The engine is still loading. Try again in a moment.")),
            Status::Error { message } => Err(err(format!("The engine is in an error state: {message}"))),
            _ => Err(err(
                "The engine is off. Open Qwen Image Studio and press Start.",
            )),
        }
    }

    /// Writes the PNG next to the other outputs and returns it as an MCP image block.
    async fn deliver(
        &self,
        png: Vec<u8>,
        prompt: &str,
        w: u32,
        h: u32,
    ) -> Result<CallToolResult, McpError> {
        let dir: PathBuf = self.state.cfg.lock().await.outputs_dir();
        std::fs::create_dir_all(&dir).ok();
        let name = format!("{}.png", chrono::Local::now().format("%Y%m%d-%H%M%S"));
        let path = dir.join(&name);
        std::fs::write(&path, &png).map_err(|e| err(format!("could not save the image: {e}")))?;

        self.state.push_gallery(GalleryItem {
            path: path.to_string_lossy().to_string(),
            prompt: prompt.to_string(),
            width: w,
            height: h,
            at: chrono::Local::now().format("%H:%M").to_string(),
        });

        let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
        Ok(CallToolResult::success(vec![
            ContentBlock::image(b64, "image/png"),
            ContentBlock::text(path.to_string_lossy().to_string()),
        ]))
    }
}

#[tool_router]
impl Qwen {
    #[tool(
        description = "Generate an image locally with Qwen-Image-2.1 on this PC's GPU. Returns the image and the path it was saved to. A 1024x1024 image takes about 70 seconds."
    )]
    async fn qwen_generate(
        &self,
        Parameters(a): Parameters<GenerateArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.require_ready().await?;
        let (w, h) = (snap(a.width.unwrap_or(1024)), snap(a.height.unwrap_or(1024)));
        let prompt = a.prompt.clone();

        self.state.set_busy(Some(format!("Generating {w}×{h}"))).await;
        let out = engine::generate(GenRequest {
            prompt: a.prompt,
            negative: a.negative_prompt.unwrap_or_default(),
            width: w,
            height: h,
            steps: a.steps.unwrap_or(20).clamp(1, 60),
            seed: a.seed.unwrap_or(-1),
            refs: vec![],
        })
        .await;
        self.state.set_busy(None).await;

        self.deliver(out.map_err(|e| err(e.to_string()))?, &prompt, w, h)
            .await
    }

    #[tool(
        description = "Edit images locally with Qwen-Image-2.1, using up to 10 reference images. Accepts absolute file paths or base64. Slower than generating: expect about 5 minutes for one 1024x1024 reference, because the vision encoder runs on the CPU."
    )]
    async fn qwen_edit(
        &self,
        Parameters(a): Parameters<EditArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.require_ready().await?;
        if a.images.is_empty() {
            return Err(err("qwen_edit needs at least one reference image."));
        }
        if a.images.len() > 10 {
            return Err(err("Qwen-Image-2.1 takes at most 10 reference images."));
        }

        let mut refs = Vec::with_capacity(a.images.len());
        for img in &a.images {
            let looks_like_path = img.len() < 1024 && (img.contains('/') || img.contains('\\'));
            if looks_like_path {
                let bytes = std::fs::read(img)
                    .map_err(|e| err(format!("could not read reference image {img}: {e}")))?;
                refs.push(base64::engine::general_purpose::STANDARD.encode(bytes));
            } else {
                // Accept data URLs as well as bare base64.
                let cleaned = img.split_once(";base64,").map(|(_, b)| b).unwrap_or(img);
                refs.push(cleaned.to_string());
            }
        }

        let (w, h) = (snap(a.width.unwrap_or(1024)), snap(a.height.unwrap_or(1024)));
        let prompt = a.prompt.clone();

        self.state.set_busy(Some("Editing".into())).await;
        let out = engine::generate(GenRequest {
            prompt: a.prompt,
            negative: String::new(),
            width: w,
            height: h,
            steps: a.steps.unwrap_or(20).clamp(1, 60),
            seed: a.seed.unwrap_or(-1),
            refs,
        })
        .await;
        self.state.set_busy(None).await;

        self.deliver(out.map_err(|e| err(e.to_string()))?, &prompt, w, h)
            .await
    }
}

#[tool_handler]
impl ServerHandler for Qwen {
    fn get_info(&self) -> ServerConfig {
        let mut info = Implementation::new("qwen-image-studio", env!("CARGO_PKG_VERSION"));
        info.title = Some("Qwen Image Studio".into());
        info.description = Some("Local Qwen-Image-2.1 on the user's own GPU".into());
        info.website_url = Some("https://github.com/limeflash/qwen-image-studio".into());

        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(info)
            .with_instructions(
                "Local Qwen-Image-2.1 running on the user's own GPU. qwen_generate makes an \
                 image from a prompt; qwen_edit changes existing images given up to 10 \
                 references. Both return the image inline and the path it was written to. \
                 Sizes are rounded to a multiple of 32. Generation takes about a minute, \
                 editing several minutes, so do not call them speculatively."
                    .to_string(),
            )
    }
}
