# Qwen Image Studio

Run [Qwen-Image-2.1](https://huggingface.co/Qwen/Qwen-Image-2.1) on your own GPU and hand it to
Claude as an MCP server. One Windows executable: it downloads the weights, runs the engine, and
serves the tools — no Python, no ComfyUI, no node graphs.


## What it is

A small Tauri 2 desktop app that:

- installs ~15 GB of weights on first run, resumable and checksum-verified
- runs [stable-diffusion.cpp](https://github.com/leejet/stable-diffusion.cpp) as a child process
  with a launch configuration that actually fits a 16 GB card
- **is itself the MCP server** (Rust, [`rmcp`](https://github.com/modelcontextprotocol/rust-sdk)
  over Streamable HTTP), exposing `qwen_generate` and `qwen_edit`
- optionally opens a Cloudflare quick tunnel so your laptop and claude.ai can reach it

The engine listens on `127.0.0.1` only. The MCP endpoint is the one thing exposed, and its URL
carries a 32-character secret — anything else gets a 404.

## Tools

| Tool | Arguments | Notes |
|---|---|---|
| `qwen_generate` | `prompt`, `width`, `height`, `steps`, `seed`, `negative_prompt`, `transparent` | ~70 s for 1024×1024 at 20 steps |
| `qwen_edit` | `prompt`, `images[]` (paths or base64), plus the same options | up to 10 references; ~5 min, the vision encoder runs on the CPU |

Both return the PNG inline and the path it was written to.

**Sizes.** Anything from 256 to 2048 per side, rounded to a multiple of 32 as the engine
requires, any aspect ratio. Measured on a 16 GB card at 8 steps: 1024² takes 28 s and peaks at
10.5 GB, 1664×928 takes 42 s, and the model's 2048² maximum takes 162 s and peaks at 12.1 GB —
still about 4 GB clear.

**Transparency.** `transparent: true` returns a cut-out RGBA PNG. Qwen-Image-2.1 writes its own
alpha and the prompt is what asks for it, so the flag simply rewrites the prompt into the form
the model card prescribes. Measured: a plain prompt yields 0 % transparent pixels, the rewritten
one 67 %. No matting model, no background removal.

## Requirements

- Windows 11, NVIDIA GPU with 12 GB+ VRAM (developed on an RTX 4070 16 GB)
- ~15 GB of disk for the weights
- [cloudflared](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/)
  only if you want the tunnel: `winget install --id Cloudflare.cloudflared`

## Connecting Claude

The panel shows both URLs with a copy button.

```bash
claude mcp add --transport http qwen-image "http://localhost:8765/<secret>/mcp"
```

For claude.ai or your phone, turn the tunnel on and paste the `https://….trycloudflare.com/…/mcp`
URL into **Settings → Connectors → Add custom connector**. The secret lives in the URL path
because claude.ai's custom-connector UI has no field for a static `Authorization` header — only
OAuth ([#112](https://github.com/anthropics/claude-ai-mcp/issues/112),
[#715](https://github.com/anthropics/claude-ai-mcp/issues/715)).

A quick tunnel gets a **new hostname every launch**, so the connector has to be re-pasted. The
panel makes that one click and then confirms the paste worked by showing you when claude.ai
actually arrived on the new hostname. For a stable hostname, use a named tunnel with your own
domain.

## Where things are installed

Two separate choices:

- **The app** — the installer asks, per-user or per-machine, with its own directory page.
  It is a few megabytes.
- **The weights** — ~15 GB, on the same drive the installer was pointed at: install to `H:` and
  they go to `H:\Qwen Image Studio`, not into Program Files, which would need admin to write.
  `Change` opens a folder picker, the free space is shown beside the path, and Download is
  refused with the shortfall spelled out if the volume cannot hold the install.

  A folder remembered from an earlier install is only reused when something is actually in it;
  an empty one on another drive is a leftover, and the app follows the installer instead.

Only the selected tier is fetched. A first launch waits for the Download button; an install
that was interrupted resumes on its own.

### If the weights will not download

The engine and CUDA runtime come from GitHub; everything else comes from Hugging Face, which a
fair number of ISPs block. When that happens each row says `failed · can't reach huggingface.co`
rather than blaming the network in general, and the app fails fast instead of spending its retry
budget on a host that is never going to answer.

A VPN is the simple fix: turn it on and press Retry. Otherwise a field appears under the failed
rows — paste a proxy that mirrors Hugging Face into it and the failed files start again straight
away, no restart. It is saved, and `HF_ENDPOINT` works too, the same variable `huggingface_hub`
honours.

No mirror is built in on purpose: `hf-mirror.com`, the usual suggestion, answers `308` straight
back to `huggingface.co`, so it is no help to anyone who cannot reach that host to begin with.

## Model tiers

Picked in the app; switching requires an engine restart.

| Tier | Size | Verdict on 16 GB |
|---|---|---|
| **Q6_K** | 6.00 GB | default — best that fits |
| Q4_K_M | 4.20 GB | smallest, visibly lower quality |
| Q8_0 | 7.69 GB | does not fit, see below |

The text encoder is [Heretic](https://huggingface.co/pottokao/Qwen-Image-2.1-Text-Encoder-Heretic-GGUF),
an abliterated Qwen3-VL-8B. Prompt refusals live in the text encoder, not in the DiT, so the tier
only trades VRAM against quality.

### Why Q8_0 does not fit

Not because of its own size. At 1024² the VAE needs ~10.8 GB for weight preparation, and next to a
7.33 GB DiT there is only 7.6 GB left. Q6_K leaves room. Two more findings from the same bring-up,
both baked into the launch arguments:

- `--offload-to-cpu` is unusable here: its staging allocation fails on CUDA 13 drivers even with
  14 GB of VRAM free — at 1020 MiB, at 969 MiB, and at 355 MiB alike. The official example in
  sd.cpp's own docs reproduces it. Weights go straight to VRAM instead.
- The text encoder must stay on the CPU. With `te=cuda0` generation works, but editing dies on the
  VAE encode of the reference image needing 4414 MB with 4413 MB free.

## Building

```bash
cargo build --release --manifest-path src-tauri/Cargo.toml
```

Needs the Rust toolchain, MSVC Build Tools, and WebView2 (shipped with Windows 11).

## Layout

```
src/            index.html, app.css, app.js — vanilla, no bundler
src-tauri/src/
  lib.rs        window, state, commands, the axum mount
  mcp.rs        the two tools
  engine.rs     sd-server lifecycle and the img_gen client
  download.rs   resumable downloads, sha256, unpacking
  tunnel.rs     cloudflared, URL parsing
docs/DESIGN.md  the full design spec
```

## Licence

MIT for this code. The weights are under the
[Qwen Research License](https://huggingface.co/Qwen/Qwen-Image-2.1/blob/main/LICENSE), which
restricts them to non-commercial research and evaluation.
