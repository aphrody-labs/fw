#![doc = "Vercel & Vercel-Labs control-plane manifest parser, Fluid Compute guards, and Pro cost estimator."]
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Errors emitted while loading or validating `ay-vercel.toml` or `vercel.json`.
#[derive(Debug, Error)]
pub enum VercelManifestError {
    /// Failed to parse TOML manifest.
    #[error("failed to parse ay-vercel.toml: {0}")]
    TomlParse(#[from] toml::de::Error),
    /// Failed to parse JSON configuration.
    #[error("failed to parse vercel.json: {0}")]
    JsonParse(#[from] serde_json::Error),
    /// Invalid project production URL.
    #[error("project `{name}` has invalid HTTPS production URL `{url}`")]
    InvalidProjectUrl {
        /// Vercel project name.
        name: String,
        /// Configured production URL.
        url: String,
    },
    /// Function `maxDuration` exceeds Fluid Compute guard.
    #[error("function route `{route}` maxDuration {seconds}s exceeds {limit}s limit")]
    MaxDurationExceeded {
        /// Route pattern key.
        route: String,
        /// Configured seconds.
        seconds: u32,
        /// Allowed ceiling.
        limit: u32,
    },
}

/// Top-level `ay-vercel.toml` configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AyVercelManifest {
    /// Product identity metadata.
    pub product: ProductSpec,
    /// Vercel plan & runtime defaults.
    pub plans: PlansSpec,
    /// Fluid Compute & bandwidth FinOps baselines.
    pub finops: VercelFinOps,
    /// Vercel projects linked to the team.
    #[serde(default)]
    pub project: Vec<ProjectSpec>,
}

/// Product identity block.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProductSpec {
    /// Monorepo name (`ay-vercel`).
    pub name: String,
    /// Canonical GitHub repository slug.
    pub repo: String,
    /// Fork destination organization.
    pub fork_org: String,
}

/// Active Vercel platform capabilities.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlansSpec {
    /// Plan tier (`hobby`, `pro`, `enterprise`).
    pub tier: String,
    /// Fluid Compute enabled flag.
    pub fluid_compute: bool,
    /// Bun runtime enabled flag.
    pub bun_runtime: bool,
    /// Agentic infrastructure flag.
    pub agentic_infra: bool,
}

/// Vercel Pro & Fluid Compute billing constants.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VercelFinOps {
    /// Base monthly cost per Pro seat (USD).
    pub pro_base_seat_usd: f64,
    /// Included Fast Data Transfer in GB.
    pub pro_included_fast_data_transfer_gb: f64,
    /// Included monthly Edge Requests.
    pub pro_included_edge_requests: u64,
    /// Overage USD per extra GB transferred.
    pub pro_extra_data_transfer_per_gb_usd: f64,
    /// Overage USD per extra 1M Edge Requests.
    pub pro_extra_edge_requests_per_million_usd: f64,
    /// USD per Fluid Compute active CPU-hour.
    pub pro_active_cpu_per_hour_usd: f64,
    /// USD per Fluid Compute provisioned memory GB-hour.
    pub pro_provisioned_memory_per_gb_hour_usd: f64,
    /// Default Fluid Compute `maxDuration` in seconds.
    pub default_max_duration_seconds: u32,
    /// Default function memory in MB.
    pub default_memory_mb: u32,
}

/// Project metadata declared in `ay-vercel.toml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectSpec {
    /// Vercel project slug.
    pub name: String,
    /// Detected framework (`nextjs`, `other`, etc.).
    pub framework: String,
    /// Optional production URL.
    pub production_url: Option<String>,
    /// Runtime major version string (`24.x`).
    pub node_version: String,
    /// Whether Fluid Compute is active.
    pub fluid_compute: bool,
}

/// Monthly Vercel Pro usage snapshot.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ProUsageSnapshot {
    /// Active Pro seats.
    pub seats: u32,
    /// Fast Data Transfer in GB.
    pub fast_data_transfer_gb: f64,
    /// Total monthly Edge Requests.
    pub edge_requests: u64,
    /// Total Fluid Compute active CPU-hours.
    pub active_cpu_hours: f64,
    /// Total Fluid Compute provisioned memory GB-hours.
    pub provisioned_memory_gb_hours: f64,
}

/// Subset of `vercel.json` validated offline.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VercelJsonConfig {
    /// Framework identifier.
    pub framework: Option<String>,
    /// Fluid Compute toggle.
    pub fluid: Option<bool>,
    /// Per-route function limits.
    #[serde(default)]
    pub functions: HashMap<String, FunctionLimit>,
}

/// Function resource caps in `vercel.json`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionLimit {
    /// Maximum execution duration in seconds.
    pub max_duration: Option<u32>,
    /// Memory allocation in MB.
    pub memory: Option<u32>,
}

impl AyVercelManifest {
    /// Parses `ay-vercel.toml` and verifies project URLs.
    ///
    /// # Errors
    /// Returns [`VercelManifestError`] if parsing or URL validation fails.
    pub fn from_toml_str(input: &str) -> Result<Self, VercelManifestError> {
        let parsed: Self = toml::from_str(input)?;
        for proj in &parsed.project {
            if let Some(url) = &proj.production_url {
                let u = Url::parse(url).map_err(|_| VercelManifestError::InvalidProjectUrl {
                    name: proj.name.clone(),
                    url: url.clone(),
                })?;
                if u.scheme() != "https" {
                    return Err(VercelManifestError::InvalidProjectUrl {
                        name: proj.name.clone(),
                        url: url.clone(),
                    });
                }
            }
        }
        Ok(parsed)
    }

    /// Estimates monthly Vercel Pro + Fluid Compute bill (USD).
    #[must_use]
    pub fn estimate_pro_monthly_usd(&self, usage: ProUsageSnapshot) -> f64 {
        let f = &self.finops;
        let seats = f64::from(usage.seats.max(1)) * f.pro_base_seat_usd;
        let extra_gb =
            (usage.fast_data_transfer_gb - f.pro_included_fast_data_transfer_gb).max(0.0);
        let extra_req = usage
            .edge_requests
            .saturating_sub(f.pro_included_edge_requests) as f64;
        seats
            + extra_gb * f.pro_extra_data_transfer_per_gb_usd
            + (extra_req / 1_000_000.0) * f.pro_extra_edge_requests_per_million_usd
            + usage.active_cpu_hours * f.pro_active_cpu_per_hour_usd
            + usage.provisioned_memory_gb_hours * f.pro_provisioned_memory_per_gb_hour_usd
    }
}

/// Validates a `vercel.json` configuration string against Fluid Compute duration limits.
///
/// # Errors
/// Returns [`VercelManifestError`] on malformed JSON or `maxDuration > 300`.
pub fn validate_vercel_json(input: &str) -> Result<VercelJsonConfig, VercelManifestError> {
    let cfg: VercelJsonConfig = serde_json::from_str(input)?;
    for (route, spec) in &cfg.functions {
        if let Some(sec) = spec.max_duration
            && sec > 300
        {
            return Err(VercelManifestError::MaxDurationExceeded {
                route: route.clone(),
                seconds: sec,
                limit: 300,
            });
        }
    }
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ay_vercel_manifest_and_web_config() -> Result<(), Box<dyn std::error::Error>> {
        let raw = include_str!("../../../ay-vercel.toml");
        let manifest = AyVercelManifest::from_toml_str(raw)?;
        assert_eq!(manifest.product.name, "ay-vercel");
        assert!(manifest.plans.fluid_compute);
        assert!(manifest.project.len() >= 3);

        let cost = manifest.estimate_pro_monthly_usd(ProUsageSnapshot {
            seats: 1,
            fast_data_transfer_gb: 200.0,
            edge_requests: 2_000_000,
            active_cpu_hours: 10.0,
            provisioned_memory_gb_hours: 100.0,
        });
        assert!(cost > 20.0);

        let vj = include_str!("../../../apps/web/vercel.json");
        let parsed = validate_vercel_json(vj)?;
        assert_eq!(parsed.fluid, Some(true));
        Ok(())
    }
}
