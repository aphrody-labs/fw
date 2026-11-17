//! The engines this browser can run on, what each one brings, and the rule that picks one.
//!
//! One runtime (Bun, our fork) embeds an engine and can switch it. An engine is a combination of a JavaScript engine, a
//! style and layout engine, a painter and a GPU layer, plus the debugging protocol it speaks. The catalog says what each
//! backend has; [`pick`] chooses the lightest one that satisfies what a page needs; [`escalate`] chooses the next heavier one
//! when the first could not do it (the lessons of `testing/acceptance.ts` say where that happens).
//!
//! Kitesurf (`third_party/kitesurf`, pinned on its 26 `browser-*` Rust engine crates: Blitz, Stylo, Taffy, Parley, Vello,
//! Anyrender, Boa) is the unified Rust browser engine behind CDP; Blink (`//headless:headless_shell` from the Chromium 160
//! checkout) is the full-compatibility reference engine.

pub mod process;

/// Where the JavaScript of a page runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Js {
    /// The engine has none of its own: the embedder provides it (JavaScriptCore through Bun, or none for a static page).
    Host,
    /// Boa (Rust ECMAScript engine, `browser-boa_engine`), run in per-page isolates by Kitesurf.
    Boa,
    JavaScriptCore,
    V8,
    SpiderMonkey,
}

/// What puts pixels on the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paint {
    None,
    WebKit,
    Skia,
    WebRender,
    Vello,
}

/// The GPU layer under the painter (and under WebGPU, when the engine has it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gpu {
    None,
    /// The operating system's (Core Animation and Metal for WebKit).
    System,
    Dawn,
    Wgpu,
}

/// How much of the Chrome DevTools Protocol the engine speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cdp {
    None,
    Subset,
    Full,
}

/// How far along the backend is in this repository.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Built and driven today.
    Shipping,
    /// Imported and tracked, not yet driven end to end.
    Experimental,
    /// Followed upstream only (`testing/acceptance.toml`, `track`).
    Watching,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Backend {
    pub id: &'static str,
    pub js: Js,
    pub style: &'static str,
    pub layout: &'static str,
    pub paint: Paint,
    pub gpu: Gpu,
    pub webgpu: bool,
    pub media: bool,
    pub cdp: Cdp,
    pub stage: Stage,
    /// The order in which backends are tried, lightest first. Not a measurement.
    pub weight: u8,
}

/// Canonical `browser-*` Rust engine crates owned by the Kitesurf workspace (`third_party/kitesurf/crates`).
pub const KITESURF_CRATES: &[&str] = &[
    "browser-anyrender",
    "browser-anyrender_svg",
    "browser-anyrender_vello_cpu",
    "browser-blitz-dom",
    "browser-blitz-html",
    "browser-blitz-paint",
    "browser-boa_ast",
    "browser-boa_engine",
    "browser-boa_gc",
    "browser-boa_interner",
    "browser-boa_parser",
    "browser-glifo",
    "browser-jpeg-encoder",
    "browser-parley",
    "browser-parley_engine",
    "browser-ratatui-image",
    "browser-selectors",
    "browser-stylo",
    "browser-stylo_dom",
    "browser-stylo_malloc_size_of",
    "browser-stylo_taffy",
    "browser-stylo_traits",
    "browser-taffy",
    "browser-usvg",
    "browser-vello_common",
    "browser-vello_cpu",
];

/// What a page needs from an engine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Needs {
    pub javascript: bool,
    pub paint: bool,
    pub webgpu: bool,
    pub media: bool,
    pub cdp: Option<Cdp>,
}

impl Backend {
    /// A backend that can only be watched is never picked; one that has no JavaScript of its own can only serve a page that
    /// needs none, or one whose host provides it.
    pub fn satisfies(&self, needs: &Needs, host_js: bool) -> bool {
        self.stage != Stage::Watching
            && (!needs.javascript || self.js != Js::Host || host_js)
            && (!needs.paint || self.paint != Paint::None)
            && (!needs.webgpu || self.webgpu)
            && (!needs.media || self.media)
            && needs.cdp.is_none_or(|c| self.cdp >= c)
    }
}

/// Every backend this repository knows, lightest first.
pub const CATALOG: &[Backend] = &[
    // Obscura: html5ever and Stylo's selectors on V8, no layout and no paint; headless DOM/script lite engine.
    Backend {
        id: "obscura",
        js: Js::V8,
        style: "stylo (selectors)",
        layout: "none",
        paint: Paint::None,
        gpu: Gpu::None,
        webgpu: false,
        media: false,
        cdp: Cdp::Subset,
        stage: Stage::Shipping,
        weight: 1,
    },
    // Kitesurf: Cloudflare's Rust browser engine on Blitz + Stylo + Taffy + Parley + Vello + Boa, speaking CDP.
    Backend {
        id: "kitesurf",
        js: Js::Boa,
        style: "browser-stylo",
        layout: "browser-blitz-dom+taffy+parley",
        paint: Paint::Vello,
        gpu: Gpu::Wgpu,
        webgpu: false,
        media: false,
        cdp: Cdp::Subset,
        stage: Stage::Shipping,
        weight: 2,
    },
    // Blitz (DioxusLabs): Stylo for style, Taffy for boxes, Parley for text, Vello for paint. No JavaScript bindings upstream.
    Backend {
        id: "blitz",
        js: Js::Host,
        style: "stylo",
        layout: "taffy+parley",
        paint: Paint::Vello,
        gpu: Gpu::Wgpu,
        webgpu: false,
        media: false,
        cdp: Cdp::None,
        stage: Stage::Experimental,
        weight: 3,
    },
    // WebKit with JavaScriptCore: Bun's native engine (WKWebView on macOS through Bun.WebView).
    Backend {
        id: "webkit",
        js: Js::JavaScriptCore,
        style: "webcore",
        layout: "webcore",
        paint: Paint::WebKit,
        gpu: Gpu::System,
        webgpu: true,
        media: true,
        cdp: Cdp::None,
        stage: Stage::Shipping,
        weight: 4,
    },
    // Servo: SpiderMonkey, Stylo, its own layout, WebRender; Firefox's remote debugging protocol, not CDP.
    Backend {
        id: "servo",
        js: Js::SpiderMonkey,
        style: "stylo",
        layout: "servo-layout",
        paint: Paint::WebRender,
        gpu: Gpu::Wgpu,
        webgpu: true,
        media: true,
        cdp: Cdp::None,
        stage: Stage::Experimental,
        weight: 5,
    },
    // Blink: V8, Skia, Dawn; the engine of record, built from the Chromium checkout (build/engines), native CDP.
    Backend {
        id: "blink",
        js: Js::V8,
        style: "blink",
        layout: "layoutng",
        paint: Paint::Skia,
        gpu: Gpu::Dawn,
        webgpu: true,
        media: true,
        cdp: Cdp::Full,
        stage: Stage::Shipping,
        weight: 6,
    },
    // The experimental Rust ecosystem, followed upstream: Rust JavaScript engines and a new from-scratch browser.
    Backend {
        id: "boa",
        js: Js::Host,
        style: "none",
        layout: "none",
        paint: Paint::None,
        gpu: Gpu::None,
        webgpu: false,
        media: false,
        cdp: Cdp::None,
        stage: Stage::Watching,
        weight: 7,
    },
    Backend {
        id: "nova",
        js: Js::Host,
        style: "none",
        layout: "none",
        paint: Paint::None,
        gpu: Gpu::None,
        webgpu: false,
        media: false,
        cdp: Cdp::None,
        stage: Stage::Watching,
        weight: 7,
    },
    Backend {
        id: "ladybird",
        js: Js::Host,
        style: "libweb",
        layout: "libweb",
        paint: Paint::Skia,
        gpu: Gpu::None,
        webgpu: false,
        media: false,
        cdp: Cdp::None,
        stage: Stage::Watching,
        weight: 8,
    },
];

pub fn backend(id: &str) -> Option<&'static Backend> {
    CATALOG.iter().find(|b| b.id == id)
}

/// The lightest backend among `available` (every one when empty) that satisfies `needs`. `host_js` says the embedder
/// provides JavaScript for engines that have none (Bun's JavaScriptCore).
pub fn pick(needs: &Needs, available: &[&str], host_js: bool) -> Option<&'static Backend> {
    CATALOG
        .iter()
        .filter(|b| available.is_empty() || available.contains(&b.id))
        .filter(|b| b.satisfies(needs, host_js))
        .min_by_key(|b| b.weight)
}

/// The next heavier backend than `from` that satisfies `needs`: what to replay the session on when `from` fell short.
pub fn escalate(
    from: &str,
    needs: &Needs,
    available: &[&str],
    host_js: bool,
) -> Option<&'static Backend> {
    let floor = backend(from)?.weight;
    CATALOG
        .iter()
        .filter(|b| b.weight > floor)
        .filter(|b| available.is_empty() || available.contains(&b.id))
        .filter(|b| b.satisfies(needs, host_js))
        .min_by_key(|b| b.weight)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn needs() -> Needs {
        Needs::default()
    }

    #[test]
    fn ids_are_unique_and_the_catalog_is_ordered() {
        for (i, a) in CATALOG.iter().enumerate() {
            assert!(
                CATALOG[i + 1..].iter().all(|b| b.id != a.id),
                "{} twice",
                a.id
            );
        }
        assert!(CATALOG.windows(2).all(|w| w[0].weight <= w[1].weight));
        assert_eq!(KITESURF_CRATES.len(), 26);
    }

    #[test]
    fn a_script_page_without_paint_goes_to_the_lite_engine() {
        let n = Needs {
            javascript: true,
            cdp: Some(Cdp::Subset),
            ..needs()
        };
        assert_eq!(pick(&n, &[], false).unwrap().id, "obscura");
    }

    #[test]
    fn a_script_page_with_paint_and_cdp_subset_goes_to_kitesurf() {
        let n = Needs {
            javascript: true,
            paint: true,
            cdp: Some(Cdp::Subset),
            ..needs()
        };
        assert_eq!(pick(&n, &[], false).unwrap().id, "kitesurf");
    }

    #[test]
    fn a_static_page_that_needs_paint_goes_to_kitesurf_first() {
        let n = Needs {
            paint: true,
            ..needs()
        };
        assert_eq!(pick(&n, &[], false).unwrap().id, "kitesurf");
        assert_eq!(pick(&n, &["blitz", "blink"], false).unwrap().id, "blitz");
    }

    #[test]
    fn blitz_takes_script_pages_only_when_the_host_runs_the_script() {
        let n = Needs {
            javascript: true,
            paint: true,
            ..needs()
        };
        assert_eq!(pick(&n, &["blitz", "blink"], false).unwrap().id, "blink");
        assert_eq!(pick(&n, &["blitz", "blink"], true).unwrap().id, "blitz");
    }

    #[test]
    fn full_cdp_means_blink() {
        let n = Needs {
            javascript: true,
            paint: true,
            cdp: Some(Cdp::Full),
            ..needs()
        };
        assert_eq!(pick(&n, &[], false).unwrap().id, "blink");
    }

    #[test]
    fn watched_engines_are_never_picked() {
        for id in ["boa", "nova", "ladybird"] {
            assert!(pick(&needs(), &[id], true).is_none(), "{id}");
        }
    }

    #[test]
    fn escalation_moves_from_kitesurf_to_blink_when_cdp_is_required() {
        let cdp_paint = Needs {
            javascript: true,
            paint: true,
            cdp: Some(Cdp::Subset),
            ..needs()
        };
        assert_eq!(escalate("obscura", &cdp_paint, &[], false).unwrap().id, "kitesurf");
        assert_eq!(escalate("kitesurf", &cdp_paint, &[], false).unwrap().id, "blink");

        let webgpu = Needs {
            javascript: true,
            paint: true,
            webgpu: true,
            ..needs()
        };
        assert_eq!(escalate("obscura", &webgpu, &[], false).unwrap().id, "webkit");
        assert_eq!(escalate("webkit", &webgpu, &[], false).unwrap().id, "servo");
        assert_eq!(escalate("servo", &webgpu, &[], false).unwrap().id, "blink");
        assert!(escalate("blink", &webgpu, &[], false).is_none());
        assert!(escalate("unknown", &webgpu, &[], false).is_none());
    }

    #[test]
    fn nothing_available_means_nothing_picked() {
        assert!(pick(&needs(), &["absent"], false).is_none());
    }
}
