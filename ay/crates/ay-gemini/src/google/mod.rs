// SPDX-License-Identifier: Apache-2.0
//! Aphrody OS Google facade: public search (SERP, discovery, cache), the language atlas and the
//! project contract, plus the OS integrations that need model clients and credentials:
//! Antigravity, Gemini, the `google` CLI, routes, detection, rate limits and translation.

#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::disallowed_macros
)]

pub mod atlas;
pub mod language_detect;
pub mod languages;
#[cfg(not(target_arch = "wasm32"))]
pub mod search;

use serde::{Deserialize, Serialize};

/// Cloud target for a workload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GoogleProject {
    Aphrody,
}

impl GoogleProject {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Aphrody => "aphrody",
        }
    }
}

/// Non-secret runtime contract shared by Rust, Bun, Python and the kernel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoogleRuntime {
    pub project: GoogleProject,
    pub location: String,
    pub home: String,
    pub auth: String,
}

impl GoogleRuntime {
    /// Resolve only non-secret configuration from the process environment.
    pub fn from_env() -> Self {
        let project = GoogleProject::Aphrody;
        Self {
            project,
            location: std::env::var("GOOGLE_CLOUD_LOCATION")
                .unwrap_or_else(|_| "us-central1".to_owned()),
            home: std::env::var("APHRODY_HOME").unwrap_or_else(|_| "~/.aphrody".to_owned()),
            auth: std::env::var("APHRODY_GCP_AUTH")
                .unwrap_or_else(|_| "adc-or-secret-manager".to_owned()),
        }
    }
}

pub use ay_model::antigravity as antigravity;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::runtime as gemini_runtime;
pub use crate::web as gemini_web;
#[cfg(not(target_arch = "wasm32"))]
pub use ay_model::client as model_client;
#[cfg(not(target_arch = "wasm32"))]
mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub use cli::cli_main;
#[cfg(not(target_arch = "wasm32"))]
pub mod detection;
#[cfg(not(target_arch = "wasm32"))]
pub mod rate_limit;
#[cfg(not(target_arch = "wasm32"))]
pub mod routes;
#[cfg(not(target_arch = "wasm32"))]
pub mod translate;

/// Create an authenticated Antigravity client from the platform credential
/// store. The token is kept inside the SDK client and is never serialized.
pub fn authenticated_client() -> Result<antigravity::AntigravityClient, antigravity::SdkError> {
    antigravity::AntigravityClient::from_credential_manager()
}

/// Create an authenticated client from a completed OAuth PKCE login and persist
/// the credentials using the platform's protected credential store (host-only: the loopback
/// redirect listener and the credential store do not exist in a browser).
#[cfg(not(target_arch = "wasm32"))]
pub async fn login() -> Result<(), antigravity::SdkError> {
    let token = antigravity::login().await?;
    antigravity::persist(&token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_ids_are_canonical() {
        assert_eq!(GoogleProject::Aphrody.id(), "aphrody");
    }
}
