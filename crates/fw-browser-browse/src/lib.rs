//! Absorbed `anomalyco/browse` capabilities in pure Rust (`third_party/browse`).
//!
//! Unifies real-browser extension pairing (`127.0.0.1:19988` relay <-> Chromium `debugger-hub`),
//! multiplexed CDP target/session pooling, protected-frame & authenticated-origin domain guardrails,
//! ethical challenge detection with strict human-handoff (never solves or bypasses anti-bot gates),
//! compact `@e1` ARIA snapshot trees with incremental diffing, redacted network capture, and WebMCP
//! (`navigator.modelContext`) page-exposed tool schemas.

use std::collections::{BTreeMap, BTreeSet};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use url::Url;

/// Default local loopback port used by the `browse` extension <-> MCP relay (`anomalyco/browse`).
pub const DEFAULT_RELAY_PORT: u16 = 19988;

/// Protected URI schemes that an agent session may never attach to, script, or screenshot.
pub const PROTECTED_SCHEMES: &[&str] = &[
    "chrome",
    "chrome-extension",
    "chrome-untrusted",
    "devtools",
    "edge",
    "brave",
    "about",
    "view-source",
    "javascript",
];

/// Sensitive HTTP headers automatically redacted during network capture (`network-capture/redaction.ts`).
pub const REDACTED_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "x-api-key",
    "x-auth-token",
    "x-csrf-token",
    "cf-access-jwt-assertion",
];

/// Pairing state between a real Chromium profile extension and the local MCP/CLI relay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionPairing {
    pub pairing_code: String,
    pub session_token: String,
    pub extension_origin: String,
    pub tab_group_id: Option<u32>,
    pub paired: bool,
}

impl ExtensionPairing {
    /// Creates a pending pairing challenge for a `chrome-extension://<id>` origin.
    pub fn initiate(pairing_code: &str, session_token: &str, extension_origin: &str) -> Result<Self, BrowseError> {
        if pairing_code.len() != 6 || !pairing_code.bytes().all(|b| b.is_ascii_digit()) {
            return Err(BrowseError::InvalidPairingCode);
        }
        if !extension_origin.starts_with("chrome-extension://") || extension_origin.len() < 24 {
            return Err(BrowseError::UntrustedExtensionOrigin(extension_origin.to_owned()));
        }
        if session_token.len() < 16 {
            return Err(BrowseError::InvalidSessionToken);
        }
        Ok(Self {
            pairing_code: pairing_code.to_owned(),
            session_token: session_token.to_owned(),
            extension_origin: extension_origin.to_owned(),
            tab_group_id: None,
            paired: false,
        })
    }

    /// Constant-time-style verification of the 6-digit pairing code from the user's browser popup.
    pub fn verify_code(&mut self, submitted_code: &str, group_id: Option<u32>) -> Result<(), BrowseError> {
        if submitted_code.len() != self.pairing_code.len() {
            return Err(BrowseError::InvalidPairingCode);
        }
        let mut diff = 0u8;
        for (a, b) in self.pairing_code.bytes().zip(submitted_code.bytes()) {
            diff |= a ^ b;
        }
        if diff != 0 {
            return Err(BrowseError::InvalidPairingCode);
        }
        self.paired = true;
        self.tab_group_id = group_id;
        Ok(())
    }
}

/// Domain access guardrail (`auth/domain-policy.ts` + `cdp/protected-frames.ts`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DomainPolicy {
    /// Allowed host patterns (`example.com`, `*.example.com`). Empty means all non-protected public origins allowed.
    pub allowlist: BTreeSet<String>,
    /// Explicitly denied host patterns (`accounts.google.com`, `*.bank.example`).
    pub denylist: BTreeSet<String>,
    /// Origins carrying active user cookies that require explicit opt-in before script execution.
    pub authenticated_origins_require_consent: bool,
    pub consented_origins: BTreeSet<String>,
}

impl DomainPolicy {
    pub fn check_url(&self, raw_url: &str, is_authenticated: bool) -> Result<(), BrowseError> {
        if raw_url == "about:blank" {
            return Ok(());
        }
        let parsed = Url::parse(raw_url).map_err(|_| BrowseError::ProtectedTarget(raw_url.to_owned()))?;
        let scheme = parsed.scheme().to_ascii_lowercase();
        if PROTECTED_SCHEMES.contains(&scheme.as_str()) {
            return Err(BrowseError::ProtectedTarget(raw_url.to_owned()));
        }
        let host = parsed
            .host_str()
            .ok_or_else(|| BrowseError::ProtectedTarget(raw_url.to_owned()))?
            .to_ascii_lowercase();

        if self.denylist.iter().any(|pat| host_matches(&host, pat)) {
            return Err(BrowseError::DomainDenied(host));
        }
        if !self.allowlist.is_empty() && !self.allowlist.iter().any(|pat| host_matches(&host, pat)) {
            return Err(BrowseError::DomainDenied(host));
        }
        if is_authenticated && self.authenticated_origins_require_consent {
            let origin = parsed.origin().ascii_serialization();
            if !self.consented_origins.contains(&origin) {
                return Err(BrowseError::AuthenticatedOriginNeedsConsent(origin));
            }
        }
        Ok(())
    }
}

fn host_matches(host: &str, pattern: &str) -> bool {
    let pat = pattern.to_ascii_lowercase();
    if let Some(suffix) = pat.strip_prefix("*.") {
        host == suffix || (host.ends_with(suffix) && host.as_bytes().get(host.len() - suffix.len() - 1) == Some(&b'.'))
    } else {
        host == pat
    }
}

/// Ethical anti-bot / 2FA / login gate handoff (`execute/challenge-detection.ts` + `handoff-notice.ts`).
///
/// When a page presents a human verification gate, SSO/MFA prompt, or payment confirmation,
/// automation MUST immediately stop and hand control to the human user—never attempting to solve or bypass.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandoffReason {
    AntiBotVerification { provider: String },
    MultiFactorAuth,
    PaymentConfirmation,
    AuthenticatedConsentRequired { origin: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffNotice {
    pub target_id: String,
    pub url: String,
    pub reason: HandoffReason,
    pub instructions: String,
}

/// Inspects page metadata/HTTP status and returns `Some(HandoffNotice)` if automation must stop and hand off.
#[must_use]
pub fn detect_handoff_gate(
    target_id: &str,
    url: &str,
    status: u16,
    title: &str,
    has_challenge_iframe_or_marker: Option<&str>,
) -> Option<HandoffNotice> {
    let lower_title = title.to_ascii_lowercase();
    if let Some(provider) = has_challenge_iframe_or_marker {
        return Some(HandoffNotice {
            target_id: target_id.to_owned(),
            url: url.to_owned(),
            reason: HandoffReason::AntiBotVerification {
                provider: provider.to_owned(),
            },
            instructions: "Verification challenge detected; automation halted. Complete it manually in the browser tab if desired.".into(),
        });
    }
    if (status == 403 || status == 429 || status == 503)
        && (lower_title.contains("just a moment")
            || lower_title.contains("attention required")
            || lower_title.contains("verify you are human")
            || lower_title.contains("security check"))
    {
        return Some(HandoffNotice {
            target_id: target_id.to_owned(),
            url: url.to_owned(),
            reason: HandoffReason::AntiBotVerification {
                provider: "http-challenge".into(),
            },
            instructions: "Verification gate returned by origin; automation halted cleanly.".into(),
        });
    }
    if lower_title.contains("two-factor") || lower_title.contains("2-step verification") || lower_title.contains("passkey") {
        return Some(HandoffNotice {
            target_id: target_id.to_owned(),
            url: url.to_owned(),
            reason: HandoffReason::MultiFactorAuth,
            instructions: "Multi-factor authentication prompt detected; waiting for human user.".into(),
        });
    }
    None
}

/// CPU and wall-clock execution budget for sandboxed `execute-realm` evaluations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionBudget {
    pub max_wall_ms: u64,
    pub max_cpu_ms: u64,
    pub max_console_bytes: usize,
    pub max_dom_nodes: usize,
}

impl Default for ExecutionBudget {
    fn default() -> Self {
        Self {
            max_wall_ms: 15_000,
            max_cpu_ms: 2_500,
            max_console_bytes: 65_536,
            max_dom_nodes: 8_192,
        }
    }
}

/// Interactive ARIA snapshot node with deterministic `@e<N>` ref selector (`snapshot/aria-snapshot.ts`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AriaSnapshotNode {
    /// Deterministic interactive ref (`"@e1"`, `"@e2"`) when actionable, or `None` for structural containers.
    pub ref_id: Option<String>,
    pub role: String,
    pub name: String,
    pub value: Option<String>,
    pub disabled: bool,
    pub focused: bool,
    pub children: Vec<AriaSnapshotNode>,
}

impl AriaSnapshotNode {
    /// Renders compact indentation-based ARIA snapshot text for LLM/MCP tools (`- button "Sign in" [@e1]`).
    #[must_use]
    pub fn format_compact(&self) -> String {
        let mut out = String::new();
        self.write_compact(&mut out, 0);
        out
    }

    fn write_compact(&self, out: &mut String, depth: usize) {
        for _ in 0..depth {
            out.push_str("  ");
        }
        out.push_str("- ");
        out.push_str(&self.role);
        if !self.name.is_empty() {
            out.push_str(" \"");
            out.push_str(&self.name.replace('"', "\\\""));
            out.push('"');
        }
        if let Some(val) = &self.value
            && !val.is_empty()
        {
            out.push_str(" value=\"");
            out.push_str(&val.replace('"', "\\\""));
            out.push('"');
        }
        if let Some(r) = &self.ref_id {
            out.push_str(" [");
            out.push_str(r);
            out.push(']');
        }
        if self.focused {
            out.push_str(" (focused)");
        }
        if self.disabled {
            out.push_str(" (disabled)");
        }
        out.push('\n');
        for child in &self.children {
            child.write_compact(out, depth + 1);
        }
    }
}

/// Assigns sequential `@e1`, `@e2`, ... refs to actionable ARIA roles in pre-order traversal.
pub fn assign_aria_refs(root: &mut AriaSnapshotNode) -> usize {
    let mut counter = 0usize;
    assign_rec(root, &mut counter);
    counter
}

fn assign_rec(node: &mut AriaSnapshotNode, counter: &mut usize) {
    let actionable = matches!(
        node.role.as_str(),
        "button"
            | "link"
            | "textbox"
            | "searchbox"
            | "combobox"
            | "checkbox"
            | "radio"
            | "switch"
            | "tab"
            | "menuitem"
            | "slider"
            | "option"
    );
    if actionable && !node.disabled {
        *counter += 1;
        node.ref_id = Some(format!("@e{}", *counter));
    } else {
        node.ref_id = None;
    }
    for child in &mut node.children {
        assign_rec(child, counter);
    }
}

/// Computes added and removed lines between two compact ARIA snapshots (`snapshot/diff-and-format.ts`).
#[must_use]
pub fn diff_snapshots(previous: &str, current: &str) -> String {
    if previous == current {
        return "(unchanged)\n".into();
    }
    let prev_set: BTreeSet<&str> = previous.lines().collect();
    let curr_set: BTreeSet<&str> = current.lines().collect();
    let mut diff = String::new();
    for removed in prev_set.difference(&curr_set) {
        diff.push_str("- ");
        diff.push_str(removed.trim());
        diff.push('\n');
    }
    for added in curr_set.difference(&prev_set) {
        diff.push_str("+ ");
        diff.push_str(added.trim());
        diff.push('\n');
    }
    diff
}

/// Redacts sensitive authentication & session headers from captured network logs.
#[must_use]
pub fn redact_headers(headers: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    headers
        .iter()
        .map(|(k, v)| {
            let lower = k.to_ascii_lowercase();
            if REDACTED_HEADERS.contains(&lower.as_str()) {
                (k.clone(), "[REDACTED]".to_owned())
            } else {
                (k.clone(), v.clone())
            }
        })
        .collect()
}

/// WebMCP tool descriptor exposed by a web page via `navigator.modelContext` (`execute/webmcp.ts`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebMcpTool {
    pub name: String,
    pub description: String,
    pub input_schema_json: String,
    pub origin: String,
}

/// Active CDP target tracked inside the multiplexed `BrowseSessionPool`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PooledTarget {
    pub target_id: String,
    pub session_id: String,
    pub browser_context_id: Option<String>,
    pub url: String,
    pub title: String,
    pub active: bool,
    pub agent_owned: bool,
    pub webmcp_tools: Vec<WebMcpTool>,
    pub handoff: Option<HandoffNotice>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BrowseError {
    #[error("pairing code must be 6 ASCII digits")]
    InvalidPairingCode,
    #[error("untrusted extension origin `{0}`")]
    UntrustedExtensionOrigin(String),
    #[error("session token must be at least 16 characters")]
    InvalidSessionToken,
    #[error("protected browser target `{0}` cannot be controlled by an agent")]
    ProtectedTarget(String),
    #[error("domain policy denied access to `{0}`")]
    DomainDenied(String),
    #[error("authenticated origin `{0}` requires user consent")]
    AuthenticatedOriginNeedsConsent(String),
    #[error("target `{0}` is paused awaiting human handoff")]
    AwaitingHumanHandoff(String),
    #[error("unknown CDP target `{0}`")]
    UnknownTarget(String),
}

/// Multiplexed CDP target, pairing, and WebMCP registry (`cdp/client-pool.ts` + `session-manager.ts`).
#[derive(Debug, Default)]
pub struct BrowseSessionPool {
    pairing: RwLock<Option<ExtensionPairing>>,
    policy: RwLock<DomainPolicy>,
    targets: RwLock<BTreeMap<String, PooledTarget>>,
}

impl BrowseSessionPool {
    #[must_use]
    pub fn new(policy: DomainPolicy) -> Self {
        Self {
            pairing: RwLock::new(None),
            policy: RwLock::new(policy),
            targets: RwLock::new(BTreeMap::new()),
        }
    }

    pub fn set_pairing(&self, pairing: ExtensionPairing) {
        *self.pairing.write() = Some(pairing);
    }

    #[must_use]
    pub fn is_paired(&self) -> bool {
        self.pairing.read().as_ref().is_some_and(|p| p.paired)
    }

    pub fn upsert_target(&self, target: PooledTarget, is_authenticated: bool) -> Result<(), BrowseError> {
        self.policy.read().check_url(&target.url, is_authenticated)?;
        self.targets.write().insert(target.target_id.clone(), target);
        Ok(())
    }

    /// Ensures a target is eligible for automated execution (not protected, not awaiting human handoff).
    pub fn ensure_executable(&self, target_id: &str) -> Result<PooledTarget, BrowseError> {
        let targets = self.targets.read();
        let target = targets
            .get(target_id)
            .cloned()
            .ok_or_else(|| BrowseError::UnknownTarget(target_id.to_owned()))?;
        if target.handoff.is_some() {
            return Err(BrowseError::AwaitingHumanHandoff(target_id.to_owned()));
        }
        self.policy.read().check_url(&target.url, false)?;
        Ok(target)
    }

    #[must_use]
    pub fn list_targets(&self) -> Vec<PooledTarget> {
        self.targets.read().values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_handshake_validates_origin_and_code() {
        let mut p = ExtensionPairing::initiate(
            "482910",
            "tok_0123456789abcdef",
            "chrome-extension://abcdefghijklmnop",
        )
        .expect("initiate ok");
        assert!(!p.paired);
        assert_eq!(p.verify_code("000000", None), Err(BrowseError::InvalidPairingCode));
        p.verify_code("482910", Some(7)).expect("code matches");
        assert!(p.paired);
        assert_eq!(p.tab_group_id, Some(7));
    }

    #[test]
    fn domain_policy_blocks_protected_schemes_and_unconsented_auth_origins() {
        let mut policy = DomainPolicy {
            authenticated_origins_require_consent: true,
            ..DomainPolicy::default()
        };
        policy.denylist.insert("*.internal.example".into());
        assert!(matches!(
            policy.check_url("chrome://settings", false),
            Err(BrowseError::ProtectedTarget(_))
        ));
        assert!(matches!(
            policy.check_url("https://payroll.internal.example/dashboard", false),
            Err(BrowseError::DomainDenied(_))
        ));
        assert!(matches!(
            policy.check_url("https://mail.example.com/inbox", true),
            Err(BrowseError::AuthenticatedOriginNeedsConsent(_))
        ));
        policy.consented_origins.insert("https://mail.example.com".into());
        assert!(policy.check_url("https://mail.example.com/inbox", true).is_ok());
    }

    #[test]
    fn challenge_detector_halts_execution_for_human_handoff() {
        let gate = detect_handoff_gate(
            "tab-1",
            "https://protected.example.com",
            403,
            "Just a moment...",
            Some("cloudflare-turnstile"),
        )
        .expect("challenge detected");
        let pool = BrowseSessionPool::default();
        pool.upsert_target(
            PooledTarget {
                target_id: "tab-1".into(),
                session_id: "s-1".into(),
                browser_context_id: None,
                url: "https://protected.example.com".into(),
                title: "Just a moment...".into(),
                active: true,
                agent_owned: true,
                webmcp_tools: vec![],
                handoff: Some(gate),
            },
            false,
        )
        .expect("upsert");
        assert_eq!(
            pool.ensure_executable("tab-1"),
            Err(BrowseError::AwaitingHumanHandoff("tab-1".into()))
        );
    }

    #[test]
    fn aria_refs_and_header_redaction_are_deterministic() {
        let mut tree = AriaSnapshotNode {
            ref_id: None,
            role: "main".into(),
            name: "Search".into(),
            value: None,
            disabled: false,
            focused: false,
            children: vec![
                AriaSnapshotNode {
                    ref_id: None,
                    role: "searchbox".into(),
                    name: "Query".into(),
                    value: Some("webgpu".into()),
                    disabled: false,
                    focused: true,
                    children: vec![],
                },
                AriaSnapshotNode {
                    ref_id: None,
                    role: "button".into(),
                    name: "Submit".into(),
                    value: None,
                    disabled: false,
                    focused: false,
                    children: vec![],
                },
            ],
        };
        assert_eq!(assign_aria_refs(&mut tree), 2);
        let text = tree.format_compact();
        assert!(text.contains("[@e1]"));
        assert!(text.contains("[@e2]"));

        let mut headers = BTreeMap::new();
        headers.insert("Authorization".into(), "Bearer secret".into());
        headers.insert("Content-Type".into(), "application/json".into());
        let clean = redact_headers(&headers);
        assert_eq!(clean.get("Authorization").map(String::as_str), Some("[REDACTED]"));
        assert_eq!(clean.get("Content-Type").map(String::as_str), Some("application/json"));
    }
}
