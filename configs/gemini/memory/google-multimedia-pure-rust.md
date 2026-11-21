# Google Ecosystem, Multimedia Stack & Pure-Rust Desktop Architecture

Durable architectural memory saved 2026-11-20 for `aphrody` (`aphrody-labs/aphrody`) and `aphrody-labs/*`.

## Core Product Direction
- **True Pure-Rust Desktop OS**: single codebase targeting **Wayland**, **Windows**, **macOS**, **WebAssembly (`wasm32-unknown-unknown` / `wasm32-wasip2`)**, and **Chromium WebGPU** (`gpui` + `gpui_wgpu` `wgpu 30` / `naga 30` + `aphrody_desktop` + `aphrody_browser`).
- **Design System**: **Material 3 Expressive (Android 17/18 + Compose M3 1.5.0)** anchored to `https://design.google/`, 3-tier tokens (`assets/os/google_design_tokens.toml`), 6-axis `Google Sans Flex`, `material-colors 0.5` Spec2025 HCT, `md-icons 0.4`, `lucide-static-svg 1.76`, `backdrop-blur-core 0.2`, `window-vibrancy 0.8`, 35 `MaterialShapes` WGSL (`assets/shaders/m3_expressive.wgsl`), WCAG 2.2 AA >= 4.5:1.
- **Zero Slop Naming**: never use `craft`, `suite`, `anything`, `souverain`, or `sovereign` in user-facing UI, copy, roadmaps, or maquettes (use upstream crate/repo names strictly in technical dependency specs).

## Pure-Rust Multimedia, Video, Photo & 3D Stack (`crates/aphrody_media` + `crates/bevy_gpui`)
- **Zero FFmpeg / Zero Chromium Headless / Zero Python Core**:
  - Containers/codecs/audio/captions/timeline/RAW/color/HDR/print from pinned `aphrody-labs/craft-libs@v0.1.0-aphrody.20261120.2` (`craft-media`, `craft-video`, `craft-audio`, `craft-audio-dsp`, `craft-captions`, `craft-time`, `craft-timeline`, `craft-aaf`, `craft-aep`, `craft-raw`, `craft-image-codecs`, `craft-raster-codecs`, `craft-color-transform`, `craft-cms`, `craft-gainmap`, `craft-metadata`, `craft-print`).
  - GPU YUV->RGB linear-premult + HDR PQ/HLG/AgX + 3D LUT + compute scopes (waveform/vectorscope/histogram) in WGSL on `wgpu 30`.
  - Live audio via `cpal 0.16` (native) / `web-sys` `AudioWorklet` (Wasm); OS HW decode (`VideoToolbox`, `Media Foundation`, `VA-API`/`NVDEC`) opt-in with pure-Rust `craft-video` fallback.
  - Programmatic video (HyperFrames migrated, `remotion` is the remaining reference): declarative JSON/TOML `Composition` (tracks, clips, keyframes with springs, transitions, captions, `{{variables}}`) rendered frame-exact in `aphrody_media::{motion, motion_render}` (CPU reference, WGSL `motion_transition` on `--gpu`) to PNG frames and an H.264 MP4 through `craft-video` (`aphrody media timeline|render`, MCP `media_render_composition`); not yet covered: nested compositions, video-layer decode, audio mix, effects/grading, live preview, catalog (parity table in `aphrody/docs/aphrody/architecture/multimedia-pure-rust-migration.md`); `aphrody_nsh` TSX offscreen capture feeds `craft-video` separately.
  - Video intelligence (`watch-skill` parity): luminance scene-delta + `blake3` keyframes + SQLite FTS5 + optional local Whisper (`candle`) + Gemini multimodal + MCP `media_watch_*` in `aphrody_pilot`.
  - 3D WebGPU (`three.js` parity): extend `crates/bevy_gpui` with glTF 2.0, 3D Gaussian Splatting (WGSL radix sort), PMREM IBL (`craft-exr`/`craft-hdr`), 3D LUT `.cube`, Rust shader-node graph -> `naga 30` IR -> WGSL/SPIR-V/HLSL/MSL.

## Pure-Rust Google Ecosystem, Workspace, OAuth (`gcloud`) & YouTube / YouTube Music (`crates/aphrody_google`)
- **Unified Auth Chain (`aphrody_google::auth`)**:
  1. OAuth 2.0 PKCE loopback reusing `crates/oauth_callback_server`,
  2. OAuth 2.0 Device Flow (headless agents / TV),
  3. Official `google-cloud-auth 1.19` (ADC / Service Account / metadata),
  4. Automatic fallback to `gcloud auth print-access-token` + `gcloud config get-value project` + `x-goog-user-project` whenever `GOOGLE_APPLICATION_CREDENTIALS` is missing or broken.
- **Full Official Google Cloud & AI Surface (`aphrody_google::ai`)**:
  - Gemini Developer API (`generativelanguage.googleapis.com`) + Vertex AI (`google-cloud-aiplatform-v1 1.19`) with OAuth Bearer (also wired into `crates/google_ai` & `crates/language_models/src/provider/google.rs`),
  - Imagen, Veo, Lyria, Gemini Live WebSocket, Embeddings, Speech-to-Text v2 (`google-cloud-speech-v2 1.16`), Text-to-Speech (`google-cloud-texttospeech-v1 1.17`), Vision (`google-cloud-vision-v1 1.14`), Video Intelligence, Translation, Gemini Computer Use (coupled to `aphrody_pilot` AccessKit).
- **Full Google Workspace in Pure Rust (`aphrody_google::workspace`)**:
  - Dynamic Discovery Service client modeled on official Rust `googleworkspace/cli` (`google-workspace-cli 0.22.5`, forked as `aphrody-labs/google-workspace-cli` to avoid collision with `aphrody-labs/cli` -> `cli/cli`) covering Drive, Gmail, Calendar, Sheets, Docs, Slides, Tasks, Chat, and MCP tools (`ws_*`).
- **Dual YouTube & YouTube Music Clients**:
  - Data API v3 + Analytics + Upload (`youtube.googleapis.com`) + Pure-Rust InnerTube (`WEB`, `WEB_REMIX`, `ANDROID_VR`, `TVHTML5`, `ytmapi-rs 0.3.4`, `rustypipe 0.11.4`, workspace JS signature solver + PO-Token + SponsorBlock + `yt-dlp` fallback ABI).
  - **Headless Agent Client**: `aphrody_cli` (`aphrody {google, ws, yt, ytm, media}` JSON/NDJSON) + `aphrody_pilot` MCP (`google_*`, `ws_*`, `yt_*`, `ytm_*`, `media_*`) + A2A 1.0 (`a2a-rs 0.10.5`) + ADK patterns (`adk-rust 2.3.0`).
  - **Human GPUI Clients**: `aphrody_desktop` `WindowKind::{YouTube, YouTubeMusic, MediaPlayer, MediaStudio, GoogleWorkspace}` with Liquid Glass, dynamic artwork HCT seed (`material-colors 0.5`), synced lyrics, gapless Opus/AAC audio, and live Gemini summaries.

## Auto-Merged Reference Forks (`aphrody-labs/*` — Zero Local Submodules)
- Never add heavy upstreams (`remotion` 4.48 GB, `three.js` 1.71 GB, `google-cloud-rust` 389 MB, `mcp-toolbox` 1.46 GB) as `.gitmodules`.
- Track them under `referenceForks` in `tooling/bun/forks-manifest.json`, audit and auto-merge daily via `tooling/bun/multimedia-forks-audit.ts --sync` and `.github/workflows/aphrody-upstream-sync.yml`.
- Cross-language Google SDK equivalence in pure Rust: Rust (`google-cloud-rust`, `googleworkspace/cli`, `adk-rust`, `gemini-rust`, `a2a-rs`, `youtui`, `rustypipe`, `google-apis-rs`, `omniget`, `libreget`, `sonora`), Go (`go-genai`, `adk-go`, `mcp-toolbox`, `gogcli`, `rclone`, `kkdai/youtube`), TypeScript (`gemini-cli`, `js-genai`, `adk-js`, `a2ui`, `genkit`, `live-api-web-console`, `stitch-sdk`, `YouTube.js`, `pear-desktop`), Python (`python-genai`, `adk-python`, `gemini-skills`, `computer-use-preview`, `A2A`, `yt-dlp`, `ytmusicapi`, `google_workspace_mcp`), Kotlin/JVM (`java-genai`, `adk-java`, `material-components-android`).
