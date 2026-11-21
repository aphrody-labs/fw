//! Chromium `//ui/views` and `//chrome/browser/ui/views` model for GPUI and headless hosts.
//!
//! Mirrors Chromium's `BrowserView` widget tree (`TopContainerView` -> `TabStripRegionView`,
//! `ToolbarView` / `LocationBarView` / `OmniboxView`, `BookmarkBarView`, `SidePanelCoordinator`,
//! `ContentsWebView`, `FindBarHost`, `StatusBubbleViews`) while embedding:
//! - multi-engine backend selection & escalation (`browser-engine`: Kitesurf -> Servo -> Blink),
//! - per-tab / per-context WireGuard/Mullvad VPN shield (`browser-vpn`: `RouteMode`),
//! - `anomalyco/browse` real-browser agent pairing, WebMCP tools, and human-handoff banners (`browser-browse`).

use browser_browse::{HandoffNotice, WebMcpTool};
use browser_engine::{Cdp, Needs};
use browser_vpn::{RouteMode, TunnelState};
use serde::{Deserialize, Serialize};
use url::Url;

/// Canonical Chromium `ViewID` identifiers (`//chrome/browser/ui/view_ids.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ViewId {
    BrowserView,
    TopContainer,
    TabStripRegion,
    TabStrip,
    NewTabButton,
    Toolbar,
    BackButton,
    ForwardButton,
    ReloadButton,
    LocationBar,
    OmniboxTextField,
    VpnShieldChip,
    BrowseAgentChip,
    BookmarkBar,
    MainContentSplit,
    ContentsWebView,
    DevToolsWebView,
    SidePanel,
    FindBar,
    HandoffInfobar,
    StatusBubble,
}

/// Axis-aligned pixel rectangle computed by `BrowserView::compute_layout`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ViewRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ViewRect {
    #[inline]
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width: if width < 0.0 { 0.0 } else { width },
            height: if height < 0.0 { 0.0 } else { height },
        }
    }
}

/// Computed Chromium `BrowserView` geometry in logical DIPs (60/120Hz zero-alloc layout pass).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BrowserLayout {
    pub window: ViewRect,
    pub tab_strip: ViewRect,
    pub toolbar: ViewRect,
    pub omnibox: ViewRect,
    pub bookmark_bar: Option<ViewRect>,
    pub handoff_infobar: Option<ViewRect>,
    pub contents_webview: ViewRect,
    pub secondary_split_webview: Option<ViewRect>,
    pub side_panel: Option<ViewRect>,
    pub find_bar: Option<ViewRect>,
    pub status_bubble: ViewRect,
}

/// Side panel surfaces hosted by Chromium's `SidePanelCoordinator`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SidePanelEntry {
    /// `anomalyco/browse` live agent relay, `@e1` ARIA snapshot & pairing controls.
    BrowseAgent,
    /// `navigator.modelContext` WebMCP tools exposed by the active tab.
    WebMcp,
    /// Fused `ayvpn` Mullvad/WireGuard exit relay selector, DAITA & DNS shield toggles.
    VpnShield,
    /// Embedded CDP DevTools inspector.
    DevTools,
    /// Bookmarks and Reading List.
    Bookmarks,
}

/// Omnibox input classification (`//components/omnibox/browser/autocomplete_input.h`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OmniboxIntent {
    NavigateUrl(String),
    DefaultSearch { query: String },
    AiModePrompt { prompt: String },
    SwitchToTab { filter: String },
    SelectVpnExit { exit_id: String },
}

/// Classifies raw user input from the Omnibox into navigation, Aphrody Search, AI mode, or `@vpn`/`@tab` commands.
#[must_use]
pub fn parse_omnibox_input(input: &str, default_search_base: &str) -> OmniboxIntent {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix("@ai ").or_else(|| trimmed.strip_prefix("/ai ")) {
        return OmniboxIntent::AiModePrompt {
            prompt: rest.trim().to_owned(),
        };
    }
    if let Some(rest) = trimmed.strip_prefix("@vpn ") {
        return OmniboxIntent::SelectVpnExit {
            exit_id: rest.trim().to_ascii_lowercase(),
        };
    }
    if let Some(rest) = trimmed.strip_prefix("@tabs ") {
        return OmniboxIntent::SwitchToTab {
            filter: rest.trim().to_owned(),
        };
    }
    if Url::parse(trimmed).is_ok() {
        return OmniboxIntent::NavigateUrl(trimmed.to_owned());
    }
    if !trimmed.contains(' ') && (trimmed.contains('.') || trimmed.starts_with("localhost")) {
        return OmniboxIntent::NavigateUrl(format!("https://{trimmed}"));
    }
    let _ = default_search_base;
    OmniboxIntent::DefaultSearch {
        query: trimmed.to_owned(),
    }
}

/// Visual tab group metadata (`//components/tab_groups`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabGroup {
    pub id: u32,
    pub title: String,
    pub color_hex: u32,
    pub collapsed: bool,
    pub agent_paired: bool,
}

/// Shipping engine shortlist passed to `browser_engine::pick` and `browser_engine::escalate`.
pub const SHIPPING_ENGINES: &[&str] = &["obscura", "kitesurf", "servo", "blink"];

/// Single tab entry inside `TabStripModel`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabModel {
    pub id: u64,
    pub title: String,
    pub url: String,
    pub pinned: bool,
    pub hibernated: bool,
    pub loading: bool,
    pub group_id: Option<u32>,
    /// Engine backend currently rendering this tab (`"kitesurf"`, `"servo"`, `"blink"`).
    pub engine_id: String,
    /// Per-tab VPN routing mode (`SystemTunnel`, `PinnedExit("jp")`, `SplitBypass`).
    pub vpn_route: RouteMode,
    /// Active `anomalyco/browse` agent ownership badge.
    pub agent_controlled: bool,
    /// Ethical human-handoff notice when a challenge/MFA gate paused automation on this tab.
    pub handoff: Option<HandoffNotice>,
    /// Page-exposed WebMCP tools (`navigator.modelContext`).
    pub webmcp_tools: Vec<WebMcpTool>,
}

impl TabModel {
    #[must_use]
    pub fn new(id: u64, title: impl Into<String>, url: impl Into<String>, needs: Needs) -> Self {
        let backend = browser_engine::pick(&needs, SHIPPING_ENGINES, false).unwrap_or(&browser_engine::CATALOG[0]);
        Self {
            id,
            title: title.into(),
            url: url.into(),
            pinned: false,
            hibernated: false,
            loading: false,
            group_id: None,
            engine_id: backend.id.to_owned(),
            vpn_route: RouteMode::SystemTunnel,
            agent_controlled: false,
            handoff: None,
            webmcp_tools: Vec::new(),
        }
    }

    /// Escalates the tab's rendering backend when a page requires heavier compatibility (`Kitesurf -> Servo -> Blink`).
    pub fn escalate_engine(&mut self, needs: Needs) -> bool {
        if let Some(next) = browser_engine::escalate(&self.engine_id, &needs, SHIPPING_ENGINES, false) {
            self.engine_id = next.id.to_owned();
            true
        } else {
            false
        }
    }
}

/// Complete state of a Chromium `BrowserView` window, shared identically between GPUI desktop views and headless runners.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserViewModel {
    pub tabs: Vec<TabModel>,
    pub groups: Vec<TabGroup>,
    pub active_tab_index: usize,
    pub split_tab_index: Option<usize>,
    pub omnibox_text: String,
    pub omnibox_focused: bool,
    pub bookmark_bar_visible: bool,
    pub side_panel: Option<SidePanelEntry>,
    pub side_panel_width: f32,
    pub find_bar_query: Option<String>,
    pub status_bubble_text: Option<String>,
    pub vpn_state: TunnelState,
    pub vpn_active_exit: String,
    pub browse_extension_paired: bool,
    next_tab_id: u64,
}

impl Default for BrowserViewModel {
    fn default() -> Self {
        Self::new_default_window()
    }
}

impl BrowserViewModel {
    pub const TAB_STRIP_HEIGHT: f32 = 38.0;
    pub const TOOLBAR_HEIGHT: f32 = 42.0;
    pub const BOOKMARK_BAR_HEIGHT: f32 = 28.0;
    pub const HANDOFF_INFOBAR_HEIGHT: f32 = 36.0;

    #[must_use]
    pub fn new_default_window() -> Self {
        let initial_needs = Needs {
            javascript: true,
            paint: true,
            webgpu: false,
            media: false,
            cdp: Some(Cdp::Subset),
        };
        let home = TabModel::new(1, "Aphrody Search", "https://www.google.com/?hl=fr", initial_needs);
        Self {
            omnibox_text: home.url.clone(),
            tabs: vec![home],
            groups: Vec::new(),
            active_tab_index: 0,
            split_tab_index: None,
            omnibox_focused: false,
            bookmark_bar_visible: true,
            side_panel: Some(SidePanelEntry::BrowseAgent),
            side_panel_width: 340.0,
            find_bar_query: None,
            status_bubble_text: None,
            vpn_state: TunnelState::Connected,
            vpn_active_exit: "us".into(),
            browse_extension_paired: true,
            next_tab_id: 2,
        }
    }

    #[must_use]
    pub fn active_tab(&self) -> Option<&TabModel> {
        self.tabs.get(self.active_tab_index)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut TabModel> {
        self.tabs.get_mut(self.active_tab_index)
    }

    pub fn open_tab(&mut self, title: impl Into<String>, url: impl Into<String>, vpn_route: RouteMode) -> u64 {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let mut tab = TabModel::new(
            id,
            title,
            url,
            Needs {
                javascript: true,
                paint: true,
                webgpu: false,
                media: false,
                cdp: Some(Cdp::Subset),
            },
        );
        tab.vpn_route = vpn_route;
        self.omnibox_text = tab.url.clone();
        self.tabs.push(tab);
        self.active_tab_index = self.tabs.len() - 1;
        id
    }

    pub fn close_tab(&mut self, index: usize) -> bool {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            return false;
        }
        self.tabs.remove(index);
        if self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len() - 1;
        }
        if let Some(active) = self.active_tab() {
            self.omnibox_text = active.url.clone();
        }
        true
    }

    pub fn activate_tab(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        self.active_tab_index = index;
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.hibernated = false;
            self.omnibox_text = tab.url.clone();
        }
        true
    }

    pub fn toggle_side_panel(&mut self, entry: SidePanelEntry) {
        if self.side_panel == Some(entry) {
            self.side_panel = None;
        } else {
            self.side_panel = Some(entry);
        }
    }

    /// Computes exact Chromium `//ui/views` bounds for any viewport size `(width, height)`.
    #[must_use]
    pub fn compute_layout(&self, width: f32, height: f32) -> BrowserLayout {
        let window = ViewRect::new(0.0, 0.0, width, height);
        let tab_strip = ViewRect::new(0.0, 0.0, width, Self::TAB_STRIP_HEIGHT);
        let mut top_y = Self::TAB_STRIP_HEIGHT;

        let toolbar = ViewRect::new(0.0, top_y, width, Self::TOOLBAR_HEIGHT);
        let omnibox = ViewRect::new(136.0, top_y + 5.0, (width - 310.0).max(120.0), Self::TOOLBAR_HEIGHT - 10.0);
        top_y += Self::TOOLBAR_HEIGHT;

        let bookmark_bar = if self.bookmark_bar_visible {
            let r = ViewRect::new(0.0, top_y, width, Self::BOOKMARK_BAR_HEIGHT);
            top_y += Self::BOOKMARK_BAR_HEIGHT;
            Some(r)
        } else {
            None
        };

        let handoff_infobar = if self.active_tab().and_then(|t| t.handoff.as_ref()).is_some() {
            let r = ViewRect::new(0.0, top_y, width, Self::HANDOFF_INFOBAR_HEIGHT);
            top_y += Self::HANDOFF_INFOBAR_HEIGHT;
            Some(r)
        } else {
            None
        };

        let content_h = (height - top_y).max(0.0);
        let panel_w = if self.side_panel.is_some() {
            self.side_panel_width.min((width * 0.45).max(0.0))
        } else {
            0.0
        };
        let side_panel = self
            .side_panel
            .map(|_| ViewRect::new(width - panel_w, top_y, panel_w, content_h));

        let web_total_w = (width - panel_w).max(0.0);
        let (contents_webview, secondary_split_webview) = if self.split_tab_index.is_some() {
            let half = web_total_w * 0.5;
            (
                ViewRect::new(0.0, top_y, half, content_h),
                Some(ViewRect::new(half, top_y, web_total_w - half, content_h)),
            )
        } else {
            (ViewRect::new(0.0, top_y, web_total_w, content_h), None)
        };

        let find_bar = self
            .find_bar_query
            .as_ref()
            .map(|_| ViewRect::new((web_total_w - 340.0).max(8.0), top_y + 8.0, 320.0, 36.0));
        let status_bubble = ViewRect::new(8.0, (height - 26.0).max(top_y), 360.0_f32.min(web_total_w), 22.0);

        BrowserLayout {
            window,
            tab_strip,
            toolbar,
            omnibox,
            bookmark_bar,
            handoff_infobar,
            contents_webview,
            secondary_split_webview,
            side_panel,
            find_bar,
            status_bubble,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omnibox_classifies_urls_ai_mode_vpn_and_search() {
        assert_eq!(
            parse_omnibox_input("aphrody.com/docs", "https://www.google.com/search?q="),
            OmniboxIntent::NavigateUrl("https://aphrody.com/docs".into())
        );
        assert_eq!(
            parse_omnibox_input("@ai compare kitesurf and blink", "https://www.google.com/search?q="),
            OmniboxIntent::AiModePrompt {
                prompt: "compare kitesurf and blink".into()
            }
        );
        assert_eq!(
            parse_omnibox_input("@vpn jp", "https://www.google.com/search?q="),
            OmniboxIntent::SelectVpnExit { exit_id: "jp".into() }
        );
        assert_eq!(
            parse_omnibox_input("rust webgpu layout", "https://www.google.com/search?q="),
            OmniboxIntent::DefaultSearch {
                query: "rust webgpu layout".into()
            }
        );
    }

    #[test]
    fn tab_escalation_moves_from_kitesurf_to_servo_to_blink() {
        let mut model = BrowserViewModel::new_default_window();
        assert_eq!(model.active_tab().expect("tab").engine_id, "kitesurf");
        let need_webgpu = Needs {
            javascript: true,
            paint: true,
            webgpu: true,
            media: false,
            cdp: None,
        };
        assert!(model.active_tab_mut().expect("tab").escalate_engine(need_webgpu));
        assert_eq!(model.active_tab().expect("tab").engine_id, "servo");
        let need_full_cdp = Needs {
            javascript: true,
            paint: true,
            webgpu: true,
            media: true,
            cdp: Some(Cdp::Full),
        };
        assert!(model.active_tab_mut().expect("tab").escalate_engine(need_full_cdp));
        assert_eq!(model.active_tab().expect("tab").engine_id, "blink");
    }

    #[test]
    fn chromium_views_layout_accounts_for_side_panel_and_split_view() {
        let mut model = BrowserViewModel::new_default_window();
        let _jp_tab = model.open_tab("Tokyo Crawl", "https://example.jp", RouteMode::PinnedExit("jp".into()));
        model.split_tab_index = Some(0);
        let layout = model.compute_layout(1440.0, 900.0);
        assert!(layout.bookmark_bar.is_some());
        assert!(layout.side_panel.is_some());
        assert!(layout.secondary_split_webview.is_some());
        let sp = layout.side_panel.expect("side panel");
        assert_eq!(sp.width, 340.0);
        assert_eq!(layout.contents_webview.width + layout.secondary_split_webview.expect("split").width, 1100.0);
    }
}
