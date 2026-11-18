// SPDX-License-Identifier: Apache-2.0
//! Headless Browser + Fused `ayvpn` + `anomalyco/browse` SDK crate (`ay-headless`).
//!
//! Integrates `aphrody-labs/browser` (`browser-engine`, `browser-vpn`, `browser-browse`, `browser-views`,
//! `browser-detect`) into `ay` for headless automation, crawlers, CI verification, and edge Dynamic Workers,
//! while the full GPUI Chromium `//ui/views` browser lives in `aphrody-dev/aphrody` (`aphrody_browser`).

use browser_browse::{
    AriaSnapshotNode, BrowseError, BrowseSessionPool, DomainPolicy, HandoffNotice, PooledTarget,
    assign_aria_refs, detect_handoff_gate,
};
use browser_engine::{Backend, Needs};
use browser_vpn::{BrowserVpnRouter, ContextProxyConfig, RouteMode, TunnelState, VpnRouteError};
use serde::{Deserialize, Serialize};
use url::Url;

pub use browser_browse as browse;
pub use browser_detect as detect;
pub use browser_engine as engine;
pub use browser_views as views;
pub use browser_vpn as vpn;

/// Launch specification synthesized for a headless browser context (`kitesurf`, `servo`, or `blink`/`ay-chromium`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadlessLaunchPlan {
    pub backend_id: String,
    pub url: String,
    pub exit_id: Option<String>,
    pub chromium_args: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HeadlessError {
    #[error(transparent)]
    Vpn(#[from] VpnRouteError),
    #[error(transparent)]
    Browse(#[from] BrowseError),
    #[error("invalid URL `{0}`")]
    InvalidUrl(String),
    #[error("no shipping headless engine satisfies {0:?}")]
    NoEngine(Needs),
}

/// High-throughput headless orchestrator combining multi-engine escalation, per-context WireGuard/Mullvad
/// exit routing, SSRF protection, `@e1` ARIA snapshot indexing, and ethical challenge handoff.
#[derive(Debug)]
pub struct HeadlessBrowserSdk {
    pub vpn: BrowserVpnRouter,
    pub sessions: BrowseSessionPool,
    pub shipping_backends: Vec<&'static str>,
}

impl Default for HeadlessBrowserSdk {
    fn default() -> Self {
        Self::new(DomainPolicy::default())
    }
}

impl HeadlessBrowserSdk {
    #[must_use]
    pub fn new(policy: DomainPolicy) -> Self {
        let vpn = BrowserVpnRouter::new();
        vpn.set_state(TunnelState::Connected);
        Self {
            vpn,
            sessions: BrowseSessionPool::new(policy),
            shipping_backends: vec!["obscura", "kitesurf", "servo", "blink"],
        }
    }

    /// Picks the lightest engine (`obscura` -> `kitesurf` -> `servo` -> `blink`) and synthesizes
    /// isolated Mullvad/WireGuard proxy flags (auto-routing `.jp` to Tokyo `"jp"` and Google to Ashburn `"us"`).
    pub fn plan_navigation(
        &self,
        raw_url: &str,
        needs: Needs,
        explicit_route: Option<RouteMode>,
    ) -> Result<HeadlessLaunchPlan, HeadlessError> {
        self.sessions.ensure_url_allowed(raw_url)?;
        let parsed =
            Url::parse(raw_url).map_err(|_| HeadlessError::InvalidUrl(raw_url.to_owned()))?;
        let backend: &Backend = browser_engine::pick(&needs, &self.shipping_backends, true)
            .ok_or(HeadlessError::NoEngine(needs))?;

        let route = match explicit_route {
            Some(r) => r,
            None => self
                .vpn
                .recommend_exit_for_url(&parsed)
                .map(RouteMode::PinnedExit)
                .unwrap_or(RouteMode::SystemTunnel),
        };
        let proxy: ContextProxyConfig = self.vpn.resolve_context_proxy(&route)?;
        let mut args = vec![
            "--headless=new".to_owned(),
            "--disable-gpu-sandbox".to_owned(),
        ];
        args.extend(proxy.chromium_args());

        Ok(HeadlessLaunchPlan {
            backend_id: backend.id.to_owned(),
            url: parsed.into(),
            exit_id: proxy.exit_id,
            chromium_args: args,
        })
    }

    /// Indexes an ARIA snapshot tree with `@e1`, `@e2` refs and checks whether the page triggered a human handoff gate.
    pub fn ingest_page_snapshot(
        &self,
        target_id: &str,
        url: &str,
        status: u16,
        title: &str,
        challenge_marker: Option<&str>,
        mut aria_root: AriaSnapshotNode,
    ) -> Result<(String, Option<HandoffNotice>), HeadlessError> {
        let _count = assign_aria_refs(&mut aria_root);
        let compact = aria_root.format_compact();
        let handoff = detect_handoff_gate(target_id, url, status, title, challenge_marker);
        self.sessions.upsert_target(
            PooledTarget {
                target_id: target_id.to_owned(),
                session_id: format!("headless-{target_id}"),
                browser_context_id: None,
                url: url.to_owned(),
                title: title.to_owned(),
                active: true,
                agent_owned: true,
                webmcp_tools: Vec::new(),
                handoff: handoff.clone(),
            },
            false,
        )?;
        Ok((compact, handoff))
    }
}

trait SessionPolicyExt {
    fn ensure_url_allowed(&self, raw_url: &str) -> Result<(), BrowseError>;
}

impl SessionPolicyExt for BrowseSessionPool {
    fn ensure_url_allowed(&self, raw_url: &str) -> Result<(), BrowseError> {
        DomainPolicy::default().check_url(raw_url, false)
    }
}

/// Helper returning the ordered escalation ladder `(kitesurf -> servo -> blink)` for a given `Needs`.
#[must_use]
pub fn escalation_chain(needs: Needs) -> Vec<&'static str> {
    let avail = ["obscura", "kitesurf", "servo", "blink"];
    let mut chain = Vec::new();
    if let Some(first) = browser_engine::pick(&needs, &avail, true) {
        chain.push(first.id);
        let mut cur = first.id;
        while let Some(next) = browser_engine::escalate(cur, &needs, &avail, true) {
            chain.push(next.id);
            cur = next.id;
        }
    }
    chain
}

#[cfg(test)]
mod tests {
    use browser_engine::Cdp;

    use super::*;

    #[test]
    fn auto_routes_japanese_crawl_to_tokyo_on_kitesurf() {
        let sdk = HeadlessBrowserSdk::default();
        let plan = sdk
            .plan_navigation(
                "https://www.nicovideo.jp/ranking",
                Needs {
                    javascript: true,
                    paint: true,
                    webgpu: false,
                    media: false,
                    cdp: Some(Cdp::Subset),
                },
                None,
            )
            .expect("plan ok");
        assert_eq!(plan.backend_id, "kitesurf");
        assert_eq!(plan.exit_id.as_deref(), Some("jp"));
        assert!(plan.chromium_args.iter().any(|a| a.contains("10802")));
    }

    #[test]
    fn escalation_chain_walks_lightest_to_full_chromium() {
        let chain = escalation_chain(Needs {
            javascript: true,
            paint: true,
            webgpu: false,
            media: false,
            cdp: None,
        });
        assert_eq!(chain, vec!["kitesurf", "servo", "blink"]);
    }
}
