#![doc = "Unified Next.js, Turbopack, Turborepo, and Vercel Runtime bridge optimized for Rust and Bun."]
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

use http::StatusCode;
use serde::Serialize;
use turbo_rcstr::{RcStr, rcstr};
use vercel_runtime::{Error, Request, Response, ResponseBody};

pub use next_code_frame;
pub use next_taskless;
pub use turbo_persistence;
pub use turbo_rcstr;
pub use turbo_tasks;
pub use turbo_tasks_backend;
pub use turbo_unix_path;
pub use turbopath;
pub use turborepo_ai_agents;
pub use turborepo_ci;
pub use turborepo_package_manager;
pub use turborepo_repository;
pub use turborepo_trace_lang_js;
pub use turborepo_trace_lang_rust;
pub use vercel_runtime;

/// Runtime health and capability payload served by `vercel_runtime`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeHealth {
    /// Service health flag.
    pub ok: bool,
    /// Runtime stack identifier.
    pub runtime: &'static str,
    /// JavaScript / package manager host optimized for.
    pub js_host: &'static str,
    /// Whether running inside a detected CI vendor (`turborepo-ci`).
    pub in_ci: bool,
    /// Count of unified Cargo crates wired into the stack.
    pub unified_crate_count: usize,
}

/// Canonical list of extracted `crates.io`-grade crates in the unified workspace.
pub const UNIFIED_CRATES: &[&str] = &[
    "auto-hash-map",
    "globwalk",
    "next-bun",
    "next-code-frame",
    "next-taskless",
    "turbo-bincode",
    "turbo-bitset",
    "turbo-dyn-eq-hash",
    "turbo-frozenmap",
    "turbo-persistence",
    "turbo-prehash",
    "turbo-rcstr",
    "turbo-rcstr-macros",
    "turbo-tasks",
    "turbo-tasks-backend",
    "turbo-tasks-bytes",
    "turbo-tasks-hash",
    "turbo-tasks-macros",
    "turbo-tasks-malloc",
    "turbo-unix-path",
    "turbopath",
    "turborepo-ai-agents",
    "turborepo-ci",
    "turborepo-dirs",
    "turborepo-errors",
    "turborepo-fixed-map",
    "turborepo-graph-utils",
    "turborepo-json-rewrite",
    "turborepo-lockfile-hash",
    "turborepo-lockfiles",
    "turborepo-log",
    "turborepo-package-manager",
    "turborepo-platform",
    "turborepo-rayon-compat",
    "turborepo-repository",
    "turborepo-task-id",
    "turborepo-trace-core",
    "turborepo-trace-lang-js",
    "turborepo-trace-lang-rust",
    "turborepo-trace-lockfile",
    "turborepo-unescape",
    "vercel-cli",
    "vercel-config",
    "vercel-skills",
    "vercel_runtime",
    "wax",
];

/// Normalizes a Next.js / Turbopack POSIX route or module path into an interned [`RcStr`].
#[must_use]
pub fn normalize_next_path(raw: &str) -> RcStr {
    let normalized = turbo_unix_path::unix_to_sys(raw);
    RcStr::from(normalized.replace('\\', "/"))
}

/// Returns the canonical Bun runtime identifier as a zero-cost static [`RcStr`].
#[must_use]
pub fn bun_host_tag() -> RcStr {
    rcstr!("bun-aot")
}

/// Builds a `vercel_runtime` JSON health response backed by [`ResponseBody`].
///
/// # Errors
/// Returns [`vercel_runtime::Error`] if serialization or HTTP response construction fails.
pub fn build_health_response() -> Result<Response<ResponseBody>, Error> {
    let payload = serde_json::to_string(&RuntimeHealth {
        ok: true,
        runtime: "vercel_runtime+turbo-tasks",
        js_host: "bun",
        in_ci: turborepo_ci::is_ci(),
        unified_crate_count: UNIFIED_CRATES.len(),
    })?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json; charset=utf-8")
        .header("cache-control", "no-store")
        .body(ResponseBody::from(payload))?)
}

/// Handles an incoming Vercel serverless request via `vercel_runtime`.
///
/// # Errors
/// Returns [`vercel_runtime::Error`] if response building fails.
pub async fn handle_health(_req: Request) -> Result<Response<ResponseBody>, Error> {
    build_health_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn builds_health_and_normalizes_paths() -> Result<(), Error> {
        let resp = build_health_response()?;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(bun_host_tag().as_str(), "bun-aot");
        assert_eq!(
            normalize_next_path("apps\\web\\src\\app\\page.tsx").as_str(),
            "apps/web/src/app/page.tsx"
        );
        Ok(())
    }
}
