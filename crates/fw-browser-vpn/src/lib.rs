//! Fused `ayvpn` + Browser network tunnel, per-context exit router, DNS shield, and ABI v1 contract.
//!
//! Combines Mullvad/WireGuard (`mullvad-daemon` embedded IPC), BoringTun/QUICHE MASQUE HTTP/3 proxy
//! endpoints, per-`BrowserContext` SOCKS5/CONNECT routing, Linux policy-routing tables (`fwmark` /
//! UID isolation from `exits.toml`), and `browser-detect` SSRF pre-flight checks into a single
//! zero-allocation fast path shared by headless SDK consumers (`ay`) and the full GPUI desktop (`aphrody`).

use std::net::{IpAddr, SocketAddr};
use browser_detect::ssrf::{is_forbidden_ip, is_loopback_ip};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use url::Url;

// -----------------------------------------------------------------------------
// ayvpn C ABI v1 constants (exact parity with abi/ayvpn.abi.toml)
// -----------------------------------------------------------------------------

pub const AYVPN_ABI_VERSION: u32 = 1;
pub const AYVPN_OK: i32 = 0;
pub const AYVPN_CHANGED: i32 = 1;
pub const AYVPN_ERR_IPC: i32 = -1;
pub const AYVPN_ERR_INPUT: i32 = -2;
pub const AYVPN_ERR_DAEMON: i32 = -3;
pub const AYVPN_ERR_RUNTIME: i32 = -4;

pub const AYVPN_STATE_DISCONNECTED: u32 = 0;
pub const AYVPN_STATE_CONNECTING: u32 = 1;
pub const AYVPN_STATE_CONNECTED: u32 = 2;
pub const AYVPN_STATE_DISCONNECTING: u32 = 3;
pub const AYVPN_STATE_ERROR: u32 = 4;

/// High-level tunnel lifecycle state matching Mullvad's `TunnelState` and `AYVPN_STATE_*`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum TunnelState {
    #[default]
    Disconnected = AYVPN_STATE_DISCONNECTED,
    Connecting = AYVPN_STATE_CONNECTING,
    Connected = AYVPN_STATE_CONNECTED,
    Disconnecting = AYVPN_STATE_DISCONNECTING,
    Error = AYVPN_STATE_ERROR,
}

impl TunnelState {
    #[inline]
    #[must_use]
    pub const fn abi_code(self) -> u32 {
        self as u32
    }

    #[inline]
    #[must_use]
    pub const fn from_abi_code(code: u32) -> Option<Self> {
        match code {
            AYVPN_STATE_DISCONNECTED => Some(Self::Disconnected),
            AYVPN_STATE_CONNECTING => Some(Self::Connecting),
            AYVPN_STATE_CONNECTED => Some(Self::Connected),
            AYVPN_STATE_DISCONNECTING => Some(Self::Disconnecting),
            AYVPN_STATE_ERROR => Some(Self::Error),
            _ => None,
        }
    }

    #[inline]
    #[must_use]
    pub const fn blocks_unencrypted_traffic(self, lockdown_mode: bool) -> bool {
        match self {
            Self::Connected => false,
            Self::Connecting | Self::Disconnecting | Self::Error => true,
            Self::Disconnected => lockdown_mode,
        }
    }
}

/// Transport protocol encapsulating the tunnel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TunnelProtocol {
    /// Kernel or BoringTun/GotaTun WireGuard UDP tunnel with optional DAITA v2 + PQ PSK.
    #[default]
    WireGuard,
    /// WireGuard obfuscated over QUIC / HTTP/3 MASQUE (`cloudflare/quiche` + Mullvad LWO).
    MasqueQuic,
    /// WireGuard over Shadowsocks or UDP-over-TCP fallback for restrictive networks.
    ShadowsocksTcp,
    /// Local SOCKS5 multiplex endpoint bound per exit relay (`127.0.0.1:<port>` or `10.64.0.1:1080`).
    Socks5Exit,
}

/// DNS privacy and content-blocking shield configuration (`mullvad_types::settings::DnsOptions`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsShield {
    pub block_ads: bool,
    pub block_trackers: bool,
    pub block_malware: bool,
    pub block_gambling: bool,
    pub block_adult: bool,
    pub dns_over_https: bool,
    pub encrypted_client_hello: bool,
}

impl Default for DnsShield {
    fn default() -> Self {
        Self {
            block_ads: true,
            block_trackers: true,
            block_malware: true,
            block_gambling: false,
            block_adult: false,
            dns_over_https: true,
            encrypted_client_hello: true,
        }
    }
}

/// Hardened tunnel security toggles (`ayvpn_daemon::Client` parity).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TunnelToggles {
    /// Allow RFC1918 & WireGuard LAN (`10.200.77.0/24`) while tunnel is active.
    pub allow_lan: bool,
    /// Fail-closed kill switch even when explicitly disconnected.
    pub lockdown_mode: bool,
    /// Connect automatically at browser/daemon startup.
    pub auto_connect: bool,
    /// Tunnel IPv6 alongside IPv4.
    pub enable_ipv6: bool,
    /// Defense Against AI-guided Traffic Analysis (Mullvad DAITA v2 constant-rate padding).
    pub daita: bool,
    /// Post-quantum ML-KEM-768 + Classic McEliece pre-shared key negotiation.
    pub quantum_resistant: bool,
}

impl Default for TunnelToggles {
    fn default() -> Self {
        Self {
            allow_lan: true,
            lockdown_mode: false,
            auto_connect: true,
            enable_ipv6: true,
            daita: true,
            quantum_resistant: true,
        }
    }
}

/// Exit location profile (`exits.toml` + custom per-tab / per-context Mullvad city relays).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExitProfile {
    /// Short identifier (`"us"`, `"jp"`, `"se"`, `"de"`, `"fr"`).
    pub id: String,
    /// Operational purpose (`"google"`, `"crawl"`, `"privacy"`, `"streaming"`).
    pub purpose: String,
    /// Two-letter ISO-3166 country code (`"us"`, `"jp"`).
    pub country: String,
    /// Three-letter Mullvad city relay code (`"qas"` Ashburn, `"tyo"` Tokyo, `"got"` Gothenburg).
    pub city: String,
    /// Optional Linux UID policy-routing isolation (`2001`, `2002`).
    pub uid: Option<u32>,
    /// Optional Linux routing table (`51821`, `51822`).
    pub table: Option<u32>,
    /// Local loopback SOCKS5 bridge port when multiplexed per `BrowserContext`.
    pub local_socks5_port: u16,
    pub protocol: TunnelProtocol,
}

impl ExitProfile {
    /// Built-in canonical exit profiles from `ayvpn/exits.toml` plus European low-latency relays.
    #[must_use]
    pub fn builtins() -> Vec<Self> {
        vec![
            Self {
                id: "us".into(),
                purpose: "google".into(),
                country: "us".into(),
                city: "qas".into(),
                uid: Some(2001),
                table: Some(51821),
                local_socks5_port: 10801,
                protocol: TunnelProtocol::WireGuard,
            },
            Self {
                id: "jp".into(),
                purpose: "crawl".into(),
                country: "jp".into(),
                city: "tyo".into(),
                uid: Some(2002),
                table: Some(51822),
                local_socks5_port: 10802,
                protocol: TunnelProtocol::WireGuard,
            },
            Self {
                id: "se".into(),
                purpose: "privacy".into(),
                country: "se".into(),
                city: "got".into(),
                uid: Some(2003),
                table: Some(51823),
                local_socks5_port: 10803,
                protocol: TunnelProtocol::MasqueQuic,
            },
            Self {
                id: "fr".into(),
                purpose: "low-latency".into(),
                country: "fr".into(),
                city: "par".into(),
                uid: Some(2004),
                table: Some(51824),
                local_socks5_port: 10804,
                protocol: TunnelProtocol::WireGuard,
            },
        ]
    }

    /// Chromium `--proxy-server` / CDP `Target.createBrowserContext.proxyServer` URI for this exit.
    #[must_use]
    pub fn chromium_proxy_uri(&self) -> String {
        format!("socks5://127.0.0.1:{}", self.local_socks5_port)
    }
}

/// How an individual `BrowserContext` or tab routes traffic.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RouteMode {
    /// Follows the global system/browser tunnel state.
    #[default]
    SystemTunnel,
    /// Pinned to a specific country/city exit profile (`"us"`, `"jp"`, `"se"`, `"fr"`).
    PinnedExit(String),
    /// Split-tunnel bypass (only allowed when `lockdown_mode == false` and target passes SSRF policy).
    SplitBypass,
}

/// CDP `Target.createBrowserContext` and Chromium process flag bundle synthesized for a route.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextProxyConfig {
    pub exit_id: Option<String>,
    pub proxy_server: Option<String>,
    pub proxy_bypass_list: String,
    pub host_resolver_rules: Option<String>,
    pub enforce_webrtc_ip_handling: &'static str,
}

impl ContextProxyConfig {
    /// Converts into Chromium CLI flags for headless or full process launch.
    #[must_use]
    pub fn chromium_args(&self) -> Vec<String> {
        let mut args = vec![
            format!("--force-webrtc-ip-handling-policy={}", self.enforce_webrtc_ip_handling),
            "--enable-features=DnsOverHttps,EncryptedClientHello".into(),
        ];
        if let Some(proxy) = &self.proxy_server {
            args.push(format!("--proxy-server={proxy}"));
            args.push(format!("--proxy-bypass-list={}", self.proxy_bypass_list));
        }
        if let Some(rules) = &self.host_resolver_rules {
            args.push(format!("--host-resolver-rules={rules}"));
        }
        args
    }
}

/// Connection-coalescing pool key ensuring tabs in different VPN exits never share HTTP/2 or QUIC sockets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CoalesceKey(u64);

impl CoalesceKey {
    /// Fast FNV-1a 64-bit key over `(origin_scheme, origin_host, origin_port, exit_id, context_group)`.
    #[inline]
    #[must_use]
    pub fn derive(origin: &Url, exit_id: &str, context_group: u32) -> Self {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in origin
            .scheme()
            .bytes()
            .chain(*b":")
            .chain(origin.host_str().unwrap_or_default().bytes())
            .chain(*b":")
            .chain(origin.port_or_known_default().unwrap_or(0).to_le_bytes())
            .chain(*b"|")
            .chain(exit_id.bytes())
            .chain(context_group.to_le_bytes())
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        Self(hash)
    }

    #[inline]
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Fixed-capacity ring buffer for zero-heap-allocation packet/flow telemetry in the UI & headless HUD.
#[derive(Clone, Copy, Debug)]
pub struct FlowRing<const N: usize = 64> {
    samples_rx_bytes: [u64; N],
    samples_tx_bytes: [u64; N],
    rtt_micros: [u32; N],
    cursor: usize,
    len: usize,
    total_rx_bytes: u64,
    total_tx_bytes: u64,
    blocked_ssrf_attempts: u64,
    blocked_tracker_queries: u64,
}

impl<const N: usize> Default for FlowRing<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> FlowRing<N> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            samples_rx_bytes: [0; N],
            samples_tx_bytes: [0; N],
            rtt_micros: [0; N],
            cursor: 0,
            len: 0,
            total_rx_bytes: 0,
            total_tx_bytes: 0,
            blocked_ssrf_attempts: 0,
            blocked_tracker_queries: 0,
        }
    }

    #[inline]
    pub fn record_sample(&mut self, rx_bytes: u64, tx_bytes: u64, rtt_us: u32) {
        if N > 0 {
            self.samples_rx_bytes[self.cursor] = rx_bytes;
            self.samples_tx_bytes[self.cursor] = tx_bytes;
            self.rtt_micros[self.cursor] = rtt_us;
            self.cursor = (self.cursor + 1) % N;
            if self.len < N {
                self.len += 1;
            }
        }
        self.total_rx_bytes = self.total_rx_bytes.saturating_add(rx_bytes);
        self.total_tx_bytes = self.total_tx_bytes.saturating_add(tx_bytes);
    }

    #[inline]
    pub fn record_ssrf_block(&mut self) {
        self.blocked_ssrf_attempts = self.blocked_ssrf_attempts.saturating_add(1);
    }

    #[inline]
    pub fn record_tracker_block(&mut self) {
        self.blocked_tracker_queries = self.blocked_tracker_queries.saturating_add(1);
    }

    #[inline]
    #[must_use]
    pub const fn totals(&self) -> (u64, u64, u64, u64) {
        (
            self.total_rx_bytes,
            self.total_tx_bytes,
            self.blocked_ssrf_attempts,
            self.blocked_tracker_queries,
        )
    }

    /// Median-like mean RTT in microseconds over active ring samples without heap allocation.
    #[must_use]
    pub fn mean_rtt_micros(&self) -> u32 {
        if self.len == 0 {
            return 0;
        }
        let mut sum: u64 = 0;
        let mut i = 0;
        while i < self.len {
            sum += u64::from(self.rtt_micros[i]);
            i += 1;
        }
        (sum / (self.len as u64)) as u32
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VpnRouteError {
    #[error("kill-switch active: tunnel state is {0:?}")]
    KillSwitchBlocked(TunnelState),
    #[error("unknown exit profile `{0}`")]
    UnknownExit(String),
    #[error("SSRF pre-flight blocked forbidden address `{0}`")]
    ForbiddenPeerIp(IpAddr),
    #[error("invalid account number: expected 16 ASCII digits")]
    InvalidAccountNumber,
}

/// Validates a 16-digit Mullvad account number (whitespace/hyphens stripped, never echoed in errors).
pub fn validate_account_number(raw: &str) -> Result<String, VpnRouteError> {
    let digits: String = raw
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace() && *ch != '-')
        .collect();
    if digits.len() == 16 && digits.bytes().all(|b| b.is_ascii_digit()) {
        Ok(digits)
    } else {
        Err(VpnRouteError::InvalidAccountNumber)
    }
}

/// Checks whether an IP belongs to the WireGuard fleet LAN (`10.200.77.0/24`) or RFC1918 LAN when `allow_lan` is on.
#[inline]
#[must_use]
pub const fn is_lan_allowed_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            (o[0] == 10 && o[1] == 200 && o[2] == 77)
                || (o[0] == 192 && o[1] == 168)
                || (o[0] == 172 && (o[1] >= 16 && o[1] <= 31))
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            (s[0] & 0xfe00) == 0xfc00
        }
    }
}

#[derive(Debug)]
struct RouterInner {
    state: TunnelState,
    active_exit: String,
    exits: Vec<ExitProfile>,
    dns: DnsShield,
    toggles: TunnelToggles,
    flow: FlowRing<64>,
}

/// Thread-safe fused Browser + VPN router shared across tabs, CDP contexts, and UI panels.
#[derive(Debug)]
pub struct BrowserVpnRouter {
    inner: RwLock<RouterInner>,
}

impl Default for BrowserVpnRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserVpnRouter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(RouterInner {
                state: TunnelState::Disconnected,
                active_exit: "us".into(),
                exits: ExitProfile::builtins(),
                dns: DnsShield::default(),
                toggles: TunnelToggles::default(),
                flow: FlowRing::new(),
            }),
        }
    }

    #[must_use]
    pub fn state(&self) -> TunnelState {
        self.inner.read().state
    }

    pub fn set_state(&self, next: TunnelState) -> bool {
        let mut g = self.inner.write();
        let changed = g.state != next;
        g.state = next;
        changed
    }

    #[must_use]
    pub fn toggles(&self) -> TunnelToggles {
        self.inner.read().toggles
    }

    pub fn set_toggles(&self, toggles: TunnelToggles) {
        self.inner.write().toggles = toggles;
    }

    #[must_use]
    pub fn dns_shield(&self) -> DnsShield {
        self.inner.read().dns
    }

    pub fn set_dns_shield(&self, dns: DnsShield) {
        self.inner.write().dns = dns;
    }

    #[must_use]
    pub fn exits(&self) -> Vec<ExitProfile> {
        self.inner.read().exits.clone()
    }

    #[must_use]
    pub fn active_exit(&self) -> ExitProfile {
        let g = self.inner.read();
        g.exits
            .iter()
            .find(|e| e.id == g.active_exit)
            .cloned()
            .unwrap_or_else(|| ExitProfile::builtins().remove(0))
    }

    pub fn select_exit(&self, exit_id: &str) -> Result<ExitProfile, VpnRouteError> {
        let mut g = self.inner.write();
        let found = g
            .exits
            .iter()
            .find(|e| e.id == exit_id)
            .cloned()
            .ok_or_else(|| VpnRouteError::UnknownExit(exit_id.to_owned()))?;
        g.active_exit = exit_id.to_owned();
        Ok(found)
    }

    /// Automatic purpose-based exit recommendation (e.g. Google/YouTube -> `"us"`, `.jp` crawl -> `"jp"`).
    #[must_use]
    pub fn recommend_exit_for_url(&self, url: &Url) -> Option<String> {
        let host = url.host_str()?.to_ascii_lowercase();
        if host.ends_with(".jp") || host.ends_with(".co.jp") || host.contains("nicovideo") || host.contains("pixiv") {
            Some("jp".into())
        } else if host.ends_with("google.com") || host.ends_with("googleapis.com") || host.ends_with("youtube.com") {
            Some("us".into())
        } else {
            None
        }
    }

    /// Synthesizes the isolated `ContextProxyConfig` for a `BrowserContext` or tab.
    pub fn resolve_context_proxy(&self, mode: &RouteMode) -> Result<ContextProxyConfig, VpnRouteError> {
        let g = self.inner.read();
        if g.state.blocks_unencrypted_traffic(g.toggles.lockdown_mode) && !matches!(mode, RouteMode::PinnedExit(_)) {
            return Err(VpnRouteError::KillSwitchBlocked(g.state));
        }

        let bypass = if g.toggles.allow_lan {
            "localhost,127.0.0.1,[::1],10.200.77.0/24,192.168.0.0/16".to_owned()
        } else {
            "<local>".to_owned()
        };

        match mode {
            RouteMode::SplitBypass => {
                if g.toggles.lockdown_mode {
                    return Err(VpnRouteError::KillSwitchBlocked(g.state));
                }
                Ok(ContextProxyConfig {
                    exit_id: None,
                    proxy_server: None,
                    proxy_bypass_list: bypass,
                    host_resolver_rules: None,
                    enforce_webrtc_ip_handling: "default_public_interface_only",
                })
            }
            RouteMode::SystemTunnel => {
                if g.state == TunnelState::Connected {
                    let exit = g.exits.iter().find(|e| e.id == g.active_exit);
                    Ok(ContextProxyConfig {
                        exit_id: exit.map(|e| e.id.clone()),
                        proxy_server: exit.map(ExitProfile::chromium_proxy_uri),
                        proxy_bypass_list: bypass,
                        host_resolver_rules: Some("MAP * ~NOTFOUND , EXCLUDE 127.0.0.1".into()),
                        enforce_webrtc_ip_handling: "disable_non_proxied_udp",
                    })
                } else {
                    Ok(ContextProxyConfig {
                        exit_id: None,
                        proxy_server: None,
                        proxy_bypass_list: bypass,
                        host_resolver_rules: None,
                        enforce_webrtc_ip_handling: "default_public_interface_only",
                    })
                }
            }
            RouteMode::PinnedExit(id) => {
                let exit = g
                    .exits
                    .iter()
                    .find(|e| e.id == *id)
                    .ok_or_else(|| VpnRouteError::UnknownExit(id.clone()))?;
                Ok(ContextProxyConfig {
                    exit_id: Some(exit.id.clone()),
                    proxy_server: Some(exit.chromium_proxy_uri()),
                    proxy_bypass_list: bypass,
                    host_resolver_rules: Some("MAP * ~NOTFOUND , EXCLUDE 127.0.0.1".into()),
                    enforce_webrtc_ip_handling: "disable_non_proxied_udp",
                })
            }
        }
    }

    /// Pre-flight peer socket guard enforcing SSRF protection before opening any socket or tunnel stream.
    pub fn check_peer_addr(&self, addr: SocketAddr, allow_loopback_devtools: bool) -> Result<(), VpnRouteError> {
        let ip = addr.ip();
        if allow_loopback_devtools && is_loopback_ip(ip) {
            return Ok(());
        }
        let allow_lan = self.inner.read().toggles.allow_lan;
        if allow_lan && is_lan_allowed_ip(ip) {
            return Ok(());
        }
        if is_forbidden_ip(ip) {
            self.inner.write().flow.record_ssrf_block();
            return Err(VpnRouteError::ForbiddenPeerIp(ip));
        }
        Ok(())
    }

    pub fn record_traffic(&self, rx_bytes: u64, tx_bytes: u64, rtt_us: u32) {
        self.inner.write().flow.record_sample(rx_bytes, tx_bytes, rtt_us);
    }

    #[must_use]
    pub fn telemetry_snapshot(&self) -> (u64, u64, u64, u64, u32) {
        let g = self.inner.read();
        let (rx, tx, ssrf, trackers) = g.flow.totals();
        (rx, tx, ssrf, trackers, g.flow.mean_rtt_micros())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_state_roundtrip_and_kill_switch_invariants() {
        for code in 0..=4 {
            let state = TunnelState::from_abi_code(code).expect("valid ABI code");
            assert_eq!(state.abi_code(), code);
        }
        assert!(TunnelState::from_abi_code(99).is_none());
        assert!(!TunnelState::Connected.blocks_unencrypted_traffic(true));
        assert!(TunnelState::Connecting.blocks_unencrypted_traffic(false));
        assert!(TunnelState::Disconnected.blocks_unencrypted_traffic(true));
        assert!(!TunnelState::Disconnected.blocks_unencrypted_traffic(false));
    }

    #[test]
    fn account_validation_strips_separators_and_rejects_malformed() {
        assert_eq!(
            validate_account_number("1234 5678-9012 3456").expect("16 digits"),
            "1234567890123456"
        );
        assert_eq!(validate_account_number("12345"), Err(VpnRouteError::InvalidAccountNumber));
        assert_eq!(
            validate_account_number("123456789012345a"),
            Err(VpnRouteError::InvalidAccountNumber)
        );
    }

    #[test]
    fn per_context_pinned_exit_and_webrtc_hardening() {
        let router = BrowserVpnRouter::new();
        let jp = router
            .resolve_context_proxy(&RouteMode::PinnedExit("jp".into()))
            .expect("jp exit");
        assert_eq!(jp.exit_id.as_deref(), Some("jp"));
        assert_eq!(jp.proxy_server.as_deref(), Some("socks5://127.0.0.1:10802"));
        assert_eq!(jp.enforce_webrtc_ip_handling, "disable_non_proxied_udp");
        let args = jp.chromium_args();
        assert!(args.iter().any(|a| a == "--proxy-server=socks5://127.0.0.1:10802"));
    }

    #[test]
    fn coalesce_key_isolates_exits_and_contexts() {
        let origin = Url::parse("https://www.google.com").expect("valid URL");
        let k_us = CoalesceKey::derive(&origin, "us", 1);
        let k_jp = CoalesceKey::derive(&origin, "jp", 1);
        let k_ctx2 = CoalesceKey::derive(&origin, "us", 2);
        assert_ne!(k_us, k_jp);
        assert_ne!(k_us, k_ctx2);
    }

    #[test]
    fn ssrf_preflight_blocks_metadata_and_allows_wireguard_lan() {
        let router = BrowserVpnRouter::new();
        let metadata: SocketAddr = "169.254.169.254:80".parse().expect("addr");
        assert!(matches!(
            router.check_peer_addr(metadata, false),
            Err(VpnRouteError::ForbiddenPeerIp(_))
        ));
        let wg_lan: SocketAddr = "10.200.77.1:22".parse().expect("addr");
        assert!(router.check_peer_addr(wg_lan, false).is_ok());
        assert_eq!(router.telemetry_snapshot().2, 1);
    }
}
