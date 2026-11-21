//! The page that hosts the wasm module, generated from the crate (`fwa-web-pack` writes it next to the wasm-bindgen
//! output) so its colors come from [`crate::palette`] and its module name from the package.

use crate::palette;

/// File stem wasm-bindgen derives from the package name (`fw-android-web`).
pub const MODULE: &str = "fw_android_web";

pub fn index_html() -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>fw android</title>\n<style>\nhtml, body {{ margin: 0; height: 100%; overflow: hidden; background: #{surface:06x}; color: #{on_surface:06x}; }}\ncanvas {{ display: block; width: 100vw; height: 100vh; outline: none; touch-action: none; }}\n#status {{ position: fixed; left: 16px; bottom: 16px; margin: 0; font: 14px/20px {code}; color: #{on_surface_variant:06x}; white-space: pre-wrap; }}\n#status[hidden] {{ display: none; }}\n</style>\n<canvas id=\"fwa\" tabindex=\"0\"></canvas>\n<p id=\"status\">loading</p>\n<script type=\"module\" src=\"./loader.js\"></script>\n",
        surface = palette::SURFACE,
        on_surface = palette::ON_SURFACE,
        on_surface_variant = palette::ON_SURFACE_VARIANT,
        code = palette::FONT_FAMILY_CODE,
    )
}

pub fn loader_js() -> String {
    format!(
        "import init, {{ start, probe }} from \"./{module}.js\";\n\nconst status = document.getElementById(\"status\");\nconst fail = (error) => {{\n  status.hidden = false;\n  status.textContent = String(error);\n  console.error(error);\n}};\n\nif (!navigator.gpu) {{\n  fail(\"WebGPU is not available in this browser\");\n}} else {{\n  try {{\n    await init({{ module_or_path: \"./{module}_bg.wasm\" }});\n    if (new URLSearchParams(location.search).has(\"probe\")) {{\n      window.fwaProbe = JSON.parse(await probe());\n      status.hidden = false;\n      status.textContent = JSON.stringify(window.fwaProbe, null, 1);\n    }} else {{\n      window.fwa = await start();\n    }}\n  }} catch (error) {{\n    fail(error);\n  }}\n}}\n",
        module = MODULE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_names_the_module_and_the_canvas_the_viewer_looks_up() {
        let html = index_html();
        assert!(html.contains("id=\"fwa\"") && html.contains("id=\"status\"") && html.contains("./loader.js"));
        assert!(html.contains("#15140c"));
        let script = loader_js();
        assert!(script.contains("./fw_android_web.js") && script.contains("./fw_android_web_bg.wasm"));
    }
}
