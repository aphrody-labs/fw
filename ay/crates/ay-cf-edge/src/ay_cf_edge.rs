#![doc = "Cloudflare Workers Paid, R2 Paid, Mullvad UID split-tunnel, and single-owner routing validator backed by serde + toml."]
#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors emitted while loading or verifying `ay-cf.toml`.
#[derive(Debug, Error)]
pub enum EdgeError {
    /// Failed to parse TOML manifest.
    #[error("failed to parse ay-cf.toml: {0}")]
    TomlParse(#[from] toml::de::Error),
    /// Hostname served by multiple conflicting origins/buckets.
    #[error("hostname `{hostname}` assigned to multiple owners: `{first}` and `{second}`")]
    DuplicateHostnameOwner {
        /// Conflicting hostname.
        hostname: String,
        /// First declared owner.
        first: String,
        /// Second declared owner.
        second: String,
    },
    /// Reserved Linux routing table or system UID detected in Mullvad VPN config.
    #[error("unsafe Mullvad policy route on exit `{id}`: uid={uid}, table={table}")]
    UnsafeVpnRoute {
        /// VPN exit identifier.
        id: String,
        /// Dedicated Linux UID.
        uid: u32,
        /// Dedicated kernel routing table ID.
        table: u32,
    },
}

/// Top-level `ay-cf.toml` typed configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AyCfManifest {
    /// FinOps policy and Cloudflare billing parameters.
    pub finops: FinOpsConfig,
    /// Bucket contracts indexed by bucket name.
    pub buckets: HashMap<String, BucketSpec>,
    /// Tunnel contracts indexed by tunnel name.
    pub tunnels: HashMap<String, TunnelSpec>,
    /// Mullvad VPN UID split-tunnel policy.
    pub vpn: VpnSpec,
}

/// Cloudflare Paid billing constants and target limits.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FinOpsConfig {
    /// Included requests in Workers Paid ($5/mo base).
    pub workers_included_requests: u64,
    /// Overage USD per extra 1M requests.
    pub workers_overage_per_million_usd: f64,
    /// Included CPU time in ms.
    pub workers_included_cpu_ms: u64,
    /// Overage USD per extra 1M CPU-ms.
    pub workers_cpu_per_million_ms_usd: f64,
    /// Included monthly Observability events.
    pub observability_included_events: u64,
    /// Overage USD per extra 1M Observability events.
    pub observability_overage_per_million_usd: f64,
    /// Free monthly R2 storage in GB.
    pub r2_included_gb: f64,
    /// R2 storage cost per GB-month beyond free tier.
    pub r2_storage_per_gb_usd: f64,
    /// Included Class A operations.
    pub r2_included_class_a: u64,
    /// Overage USD per extra 1M Class A ops.
    pub r2_class_a_per_million_usd: f64,
    /// Included Class B operations.
    pub r2_included_class_b: u64,
    /// Overage USD per extra 1M Class B ops.
    pub r2_class_b_per_million_usd: f64,
}

/// Declarative R2 bucket contract.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BucketSpec {
    /// Storage lifecycle tier.
    pub tier: String,
    /// Assigned Custom Domains.
    pub custom_domains: Vec<String>,
    /// Incomplete multipart upload expiration in seconds.
    pub abort_multipart_seconds: u64,
    /// Whether managed `r2.dev` public URL is enabled.
    pub public_r2_dev: bool,
}

/// Declarative `cloudflared` tunnel contract.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TunnelSpec {
    /// QUIC or HTTP/2 tunnel transport.
    pub transport: String,
    /// Expected healthy HA connector count.
    pub connectors: u32,
    /// Ingress hostnames owned by this tunnel.
    pub ingress: Vec<String>,
}

/// Mullvad split-tunnel configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VpnSpec {
    /// Provider name.
    pub provider: String,
    /// Routing enforcement mode.
    pub mode: String,
    /// Cluster WireGuard CIDR preserved outside VPN tables.
    pub preserve_mesh_cidr: String,
    /// Configured VPN exits.
    pub exit: Vec<VpnExitSpec>,
}

/// Per-region Mullvad WireGuard exit mapping.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VpnExitSpec {
    /// Short identifier (`us`, `jp`, `eu`).
    pub id: String,
    /// Dedicated Linux system UID (>= 1000).
    pub uid: u32,
    /// Dedicated kernel routing table ID (>= 51820).
    pub table: u32,
}

/// Monthly usage snapshot used for cost estimation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct UsageSnapshot {
    /// Monthly Worker HTTP requests.
    pub requests: u64,
    /// Monthly Worker CPU milliseconds.
    pub cpu_ms_total: u64,
    /// Monthly Observability events.
    pub observability_events: u64,
    /// Average R2 storage in GB.
    pub r2_storage_gb: f64,
    /// Monthly R2 Class A operations.
    pub r2_class_a_ops: u64,
    /// Monthly R2 Class B operations.
    pub r2_class_b_ops: u64,
}

impl AyCfManifest {
    /// Parses `ay-cf.toml` from text and enforces hostname + VPN routing invariants.
    ///
    /// # Errors
    /// Returns [`EdgeError`] if TOML syntax or invariants fail.
    pub fn from_toml_str(input: &str) -> Result<Self, EdgeError> {
        let parsed: Self = toml::from_str(input)?;
        parsed.verify()?;
        Ok(parsed)
    }

    /// Checks single-owner hostnames across R2 custom domains & tunnel ingress and verifies Mullvad UID isolation.
    ///
    /// # Errors
    /// Returns [`EdgeError`] on duplicate hostname ownership or unsafe UID/routing table IDs.
    pub fn verify(&self) -> Result<(), EdgeError> {
        let mut owners: HashMap<String, String> = HashMap::new();
        for (bucket, spec) in &self.buckets {
            for host in &spec.custom_domains {
                let owner = format!("r2:{bucket}");
                if let Some(prev) = owners.insert(host.to_ascii_lowercase(), owner.clone()) {
                    return Err(EdgeError::DuplicateHostnameOwner {
                        hostname: host.clone(),
                        first: prev,
                        second: owner,
                    });
                }
            }
        }
        for (tunnel, spec) in &self.tunnels {
            for host in &spec.ingress {
                let owner = format!("tunnel:{tunnel}");
                if let Some(prev) = owners.insert(host.to_ascii_lowercase(), owner.clone()) {
                    return Err(EdgeError::DuplicateHostnameOwner {
                        hostname: host.clone(),
                        first: prev,
                        second: owner,
                    });
                }
            }
        }

        let mut seen_uids = HashSet::new();
        let mut seen_tables = HashSet::new();
        for exit in &self.vpn.exit {
            let reserved_table = matches!(exit.table, 0 | 253 | 254 | 255) || exit.table < 1000;
            if exit.uid < 1000
                || reserved_table
                || !seen_uids.insert(exit.uid)
                || !seen_tables.insert(exit.table)
            {
                return Err(EdgeError::UnsafeVpnRoute {
                    id: exit.id.clone(),
                    uid: exit.uid,
                    table: exit.table,
                });
            }
        }
        Ok(())
    }

    /// Computes estimated monthly Cloudflare bill (USD) under Workers Paid + R2 Paid.
    #[must_use]
    pub fn estimate_monthly_usd(&self, usage: UsageSnapshot) -> f64 {
        let f = &self.finops;
        let extra_req = usage.requests.saturating_sub(f.workers_included_requests) as f64;
        let extra_cpu = usage.cpu_ms_total.saturating_sub(f.workers_included_cpu_ms) as f64;
        let extra_obs = usage
            .observability_events
            .saturating_sub(f.observability_included_events) as f64;
        let extra_gb = (usage.r2_storage_gb - f.r2_included_gb).max(0.0);
        let extra_a = usage.r2_class_a_ops.saturating_sub(f.r2_included_class_a) as f64;
        let extra_b = usage.r2_class_b_ops.saturating_sub(f.r2_included_class_b) as f64;

        5.0 + (extra_req / 1_000_000.0) * f.workers_overage_per_million_usd
            + (extra_cpu / 1_000_000.0) * f.workers_cpu_per_million_ms_usd
            + (extra_obs / 1_000_000.0) * f.observability_overage_per_million_usd
            + extra_gb * f.r2_storage_per_gb_usd
            + (extra_a / 1_000_000.0) * f.r2_class_a_per_million_usd
            + (extra_b / 1_000_000.0) * f.r2_class_b_per_million_usd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_live_ay_cf_manifest_and_validates_invariants()
    -> Result<(), Box<dyn std::error::Error>> {
        let raw = include_str!("../../../ay-cf.toml");
        let manifest = AyCfManifest::from_toml_str(raw)?;
        assert_eq!(manifest.buckets.len(), 11);
        assert_eq!(manifest.tunnels.len(), 3);
        assert_eq!(manifest.vpn.exit.len(), 3);

        let before = manifest.estimate_monthly_usd(UsageSnapshot {
            requests: 12_000_000,
            cpu_ms_total: 10_000_000,
            observability_events: 40_000_000,
            r2_storage_gb: 68.54,
            r2_class_a_ops: 500_000,
            r2_class_b_ops: 12_000_000,
        });
        let after = manifest.estimate_monthly_usd(UsageSnapshot {
            requests: 4_000_000,
            cpu_ms_total: 4_000_000,
            observability_events: 3_000_000,
            r2_storage_gb: 8.5,
            r2_class_a_ops: 200_000,
            r2_class_b_ops: 2_000_000,
        });
        assert!(after < before);
        assert!((after - 5.0).abs() < f64::EPSILON);
        Ok(())
    }
}
