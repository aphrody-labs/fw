//! Web UI assets. `ui/` is the single copy: `fw serve` embeds these files and the
//! Cloudflare Worker publishes the same directory as static assets.

pub const CONTROL_PLANE_HTML: &str = include_str!("../ui/index.html");
pub const CONTROL_PLANE_CSS: &str = include_str!("../ui/app.css");
pub const CONTROL_PLANE_JS: &str = include_str!("../ui/app.js");
pub const WEBGPU_HTML_HARNESS: &str = include_str!("../ui/webgpu.html");
