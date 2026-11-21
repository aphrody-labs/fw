// SPDX-License-Identifier: Apache-2.0
//! Shared SSRF deny-set and outbound IP policy across every browser transport, renderer, and agent surface.
//!
//! Zero external dependencies so `obscura-ssrf`, `browser-detect`, and host embedders can share one
//! exact RFC-compliant IPv4 / IPv6 / NAT64 / 6to4 / Teredo deny-set without drift.

use std::net::{IpAddr, Ipv4Addr};

/// True for loopback addresses including IPv4-mapped and IPv4-compatible IPv6 (`::ffff:127.0.0.1`).
#[must_use]
pub fn is_loopback_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6
                    .to_ipv4_mapped()
                    .or_else(|| v6.to_ipv4())
                    .is_some_and(|v4| v4.is_loopback())
        }
    }
}

/// Link-local & cloud metadata addresses (`169.254.169.254`, `fd00:ec2::254`, `100.100.100.200`).
#[must_use]
pub fn is_link_local_or_metadata(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_link_local() || v4 == Ipv4Addr::new(100, 100, 100, 200),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped().or_else(|| v6.to_ipv4()) {
                return is_link_local_or_metadata(IpAddr::V4(v4));
            }
            v6.is_unicast_link_local()
                || v6.segments() == [0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254]
        }
    }
}

/// True when `ip` must never be the target of an unprivileged outbound request:
/// loopback, RFC1918 private, CGNAT (`100.64.0.0/10`), link-local, broadcast, documentation,
/// benchmarking (`198.18.0.0/15`), 6to4 relay (`192.88.99.0/24`), special-purpose `192.0.0.0/24`
/// (except PCP anycast `.9` and `.10`), reserved `240.0.0.0/4`, unspecified (`0.0.0.0` / `::`),
/// multicast, IPv6 unique-local (`fc00::/7`), NAT64 (`64:ff9b::/96` & `64:ff9b:1::/48`), 6to4,
/// Teredo (`2001:0::/32`), discard (`100::/64`), and documentation (`2001:db8::/32`, `3fff::/20`).
#[must_use]
pub fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || o[0] == 0
                || (o[0] == 100 && (64..=127).contains(&o[1]))
                || (o[0] == 198 && (o[1] == 18 || o[1] == 19))
                || (o[0] == 192 && o[1] == 88 && o[2] == 99)
                || (o[0] == 192 && o[1] == 0 && o[2] == 0 && o[3] != 9 && o[3] != 10)
                || o[0] >= 240
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6.is_multicast()
            {
                return true;
            }
            if let Some(v4) = v6.to_ipv4_mapped().or_else(|| v6.to_ipv4()) {
                return is_forbidden_ip(IpAddr::V4(v4));
            }
            let s = v6.segments();
            if s[0] == 0x64 && s[1] == 0xff9b && s[2] == 0 && s[3] == 0 && s[4] == 0 && s[5] == 0
            {
                return is_forbidden_ip(IpAddr::V4(Ipv4Addr::new(
                    (s[6] >> 8) as u8,
                    s[6] as u8,
                    (s[7] >> 8) as u8,
                    s[7] as u8,
                )));
            }
            if s[0] == 0x2002 {
                return is_forbidden_ip(IpAddr::V4(Ipv4Addr::new(
                    (s[1] >> 8) as u8,
                    s[1] as u8,
                    (s[2] >> 8) as u8,
                    s[2] as u8,
                )));
            }
            if s[0] == 0x2001 && s[1] == 0 {
                let server = Ipv4Addr::new(
                    (s[2] >> 8) as u8,
                    s[2] as u8,
                    (s[3] >> 8) as u8,
                    s[3] as u8,
                );
                let client = Ipv4Addr::new(
                    !(s[6] >> 8) as u8,
                    !s[6] as u8,
                    !(s[7] >> 8) as u8,
                    !s[7] as u8,
                );
                return is_forbidden_ip(IpAddr::V4(server)) || is_forbidden_ip(IpAddr::V4(client));
            }
            (s[0] == 0x100 && s[1] == 0 && s[2] == 0 && s[3] == 0)
                || (s[0] == 0x64 && s[1] == 0xff9b && s[2] == 1)
                || (s[0] == 0x2001 && s[1] == 0x0db8)
                || (s[0] == 0x3fff && s[1] & 0xf000 == 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_private_metadata_and_tunneled_addresses() {
        for blocked in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "100.100.100.200",
            "::1",
            "::ffff:127.0.0.1",
            "64:ff9b::10.0.0.1",
            "2001:db8::1",
        ] {
            let ip: IpAddr = blocked.parse().unwrap();
            assert!(is_forbidden_ip(ip), "{blocked} should be forbidden");
        }
        for public in ["1.1.1.1", "8.8.8.8", "192.0.0.9", "2606:4700:4700::1111"] {
            let ip: IpAddr = public.parse().unwrap();
            assert!(!is_forbidden_ip(ip), "{public} should be allowed");
        }
    }

    #[test]
    fn identifies_loopback_and_metadata() {
        assert!(is_loopback_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(is_link_local_or_metadata("169.254.169.254".parse().unwrap()));
        assert!(is_link_local_or_metadata("100.100.100.200".parse().unwrap()));
        assert!(is_link_local_or_metadata("fd00:ec2::254".parse().unwrap()));
        assert!(!is_link_local_or_metadata("1.1.1.1".parse().unwrap()));
    }
}
