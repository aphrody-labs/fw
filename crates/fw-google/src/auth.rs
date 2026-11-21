//! Credentials: an explicit access token, then `gcloud auth print-access-token` (user credentials),
//! then Application Default Credentials. Tokens never reach `Debug`, `Display` or an error message.

use google_apis_common::GetToken;
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::OnceCell;
use yup_oauth2::authenticator::DefaultAuthenticator;

/// How long a `gcloud` token is reused (gcloud tokens live one hour).
const GCLOUD_TOKEN_TTL: Duration = Duration::from_secs(45 * 60);

type TokenResult = Result<Option<String>, Box<dyn std::error::Error + Send + Sync>>;

/// Where tokens come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// An explicit access token given by the caller.
    Token,
    /// `gcloud auth print-access-token`.
    Gcloud,
    /// Application Default Credentials (service account or authorized-user file).
    Adc,
    /// `gcloud` first, then ADC.
    Auto,
}

impl Source {
    /// Parses `auto`, `gcloud`, `adc` (case-insensitive).
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "gcloud" => Some(Self::Gcloud),
            "adc" => Some(Self::Adc),
            _ => None,
        }
    }
}

/// A secret string whose `Debug` never prints the value.
#[derive(Clone)]
struct Secret(Arc<str>);

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// Access-token provider handed to the generated hubs (they call [`GetToken`] per request).
#[derive(Clone)]
pub struct Credentials {
    source: Source,
    token: Option<Secret>,
    gcloud_cache: Arc<Mutex<HashMap<String, (Secret, Instant)>>>,
    adc_paths: Option<Vec<PathBuf>>,
    adc: Arc<OnceCell<Result<Arc<DefaultAuthenticator>, String>>>,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

impl Credentials {
    fn new(source: Source, token: Option<String>) -> Self {
        Self {
            source,
            token: token.map(|t| Secret(t.into())),
            gcloud_cache: Arc::default(),
            adc_paths: None,
            adc: Arc::default(),
        }
    }

    /// An explicit access token.
    pub fn token(token: impl Into<String>) -> Self {
        Self::new(Source::Token, Some(token.into()))
    }

    /// `gcloud auth print-access-token` only.
    pub fn gcloud() -> Self {
        Self::new(Source::Gcloud, None)
    }

    /// Application Default Credentials only.
    pub fn adc() -> Self {
        Self::new(Source::Adc, None)
    }

    /// A service-account or authorized-user JSON file at a known path (ADC format).
    pub fn adc_file(path: impl Into<PathBuf>) -> Self {
        Self {
            adc_paths: Some(vec![path.into()]),
            ..Self::new(Source::Adc, None)
        }
    }

    /// `gcloud`, then Application Default Credentials.
    pub fn auto() -> Self {
        Self::new(Source::Auto, None)
    }

    /// The documented order: the explicit token if there is one, else `gcloud`, else ADC.
    pub fn resolve(explicit_token: Option<String>) -> Self {
        match explicit_token.filter(|t| !t.trim().is_empty()) {
            Some(t) => Self::token(t.trim()),
            None => Self::auto(),
        }
    }

    /// Restricts the lookup to one source when the caller asked for it (`Source::Token` is ignored:
    /// a token is only ever set through [`Credentials::token`]).
    pub fn with_source(explicit_token: Option<String>, source: Source) -> Self {
        match (explicit_token.filter(|t| !t.trim().is_empty()), source) {
            (Some(t), _) => Self::token(t.trim()),
            (None, Source::Gcloud) => Self::gcloud(),
            (None, Source::Adc) => Self::adc(),
            (None, _) => Self::auto(),
        }
    }

    /// The configured source.
    pub fn source(&self) -> Source {
        self.source
    }

    async fn gcloud_token(&self, scopes: &[&str]) -> Result<String, String> {
        let key = scopes.join(",");
        if let Some((tok, at)) = self
            .gcloud_cache
            .lock()
            .map_err(|_| "gcloud token cache poisoned".to_string())?
            .get(&key)
            && at.elapsed() < GCLOUD_TOKEN_TTL
        {
            return Ok(tok.0.to_string());
        }
        let owned: Vec<String> = scopes.iter().map(|s| (*s).to_string()).collect();
        let token = tokio::task::spawn_blocking(move || run_gcloud(&owned))
            .await
            .map_err(|e| format!("gcloud: {e}"))??;
        if let Ok(mut cache) = self.gcloud_cache.lock() {
            cache.insert(key, (Secret(token.as_str().into()), Instant::now()));
        }
        Ok(token)
    }

    async fn adc_token(&self, scopes: &[&str]) -> Result<String, String> {
        let auth = self
            .adc
            .get_or_init(|| async {
                let paths = self
                    .adc_paths
                    .clone()
                    .unwrap_or_else(|| adc_candidate_paths(&|k| std::env::var(k).ok()));
                build_adc(&paths).await.map(Arc::new)
            })
            .await
            .as_ref()
            .map_err(Clone::clone)?;
        let tok = auth
            .token(scopes)
            .await
            .map_err(|e| format!("application default credentials: {e}"))?;
        tok.token()
            .map(str::to_owned)
            .ok_or_else(|| "application default credentials: no access token returned".to_string())
    }

    async fn fetch(&self, scopes: &[&str]) -> Result<String, String> {
        match self.source {
            Source::Token => self
                .token
                .as_ref()
                .map(|t| t.0.to_string())
                .ok_or_else(|| "no access token set".to_string()),
            Source::Gcloud => self.gcloud_token(scopes).await,
            Source::Adc => self.adc_token(scopes).await,
            Source::Auto => match self.gcloud_token(scopes).await {
                Ok(t) => Ok(t),
                Err(g) => self
                    .adc_token(scopes)
                    .await
                    .map_err(|a| format!("no credentials: {g}; {a}")),
            },
        }
    }
}

impl GetToken for Credentials {
    fn get_token<'a>(
        &'a self,
        scopes: &'a [&str],
    ) -> Pin<Box<dyn Future<Output = TokenResult> + Send + 'a>> {
        Box::pin(async move { self.fetch(scopes).await.map(Some).map_err(Into::into) })
    }
}

/// Arguments of `gcloud auth print-access-token` for the scopes a call needs.
pub fn gcloud_args(scopes: &[String]) -> Vec<String> {
    let mut args = vec!["auth".to_string(), "print-access-token".to_string()];
    if !scopes.is_empty() {
        args.push(format!("--scopes={}", scopes.join(",")));
    }
    args
}

/// Extracts the token from `gcloud` stdout: one non-empty line without whitespace.
pub fn parse_gcloud_output(stdout: &str) -> Result<String, String> {
    let token = stdout.trim();
    if token.is_empty() || token.contains(char::is_whitespace) {
        return Err("gcloud printed no access token".to_string());
    }
    Ok(token.to_string())
}

fn gcloud_program() -> &'static str {
    if cfg!(windows) {
        "gcloud.cmd"
    } else {
        "gcloud"
    }
}

fn run_gcloud_once(scopes: &[String]) -> Result<String, String> {
    let out = std::process::Command::new(gcloud_program())
        .args(gcloud_args(scopes))
        .output()
        .map_err(|e| format!("cannot run `{}`: {e}", gcloud_program()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let first = err
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("failed");
        return Err(format!("`gcloud auth print-access-token` failed: {first}"));
    }
    parse_gcloud_output(&String::from_utf8_lossy(&out.stdout))
}

/// Scoped first (service accounts need the API scopes); user credentials reject `--scopes`, so retry without.
fn run_gcloud(scopes: &[String]) -> Result<String, String> {
    match run_gcloud_once(scopes) {
        Ok(t) => Ok(t),
        Err(scoped) if !scopes.is_empty() => {
            run_gcloud_once(&[]).map_err(|plain| format!("{scoped} (without scopes: {plain})"))
        }
        Err(e) => Err(e),
    }
}

/// What an ADC JSON file contains, from its `type` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdcKind {
    ServiceAccount,
    AuthorizedUser,
    Other,
}

/// Classifies an ADC file by its `type` field without keeping any secret.
pub fn adc_kind(json: &str) -> Result<AdcKind, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("not JSON: {e}"))?;
    Ok(match v.get("type").and_then(|t| t.as_str()) {
        Some("service_account") => AdcKind::ServiceAccount,
        Some("authorized_user") => AdcKind::AuthorizedUser,
        _ => AdcKind::Other,
    })
}

/// ADC file locations in lookup order: `GOOGLE_APPLICATION_CREDENTIALS`, then the gcloud config
/// directory (`CLOUDSDK_CONFIG`, `%APPDATA%\gcloud` on Windows, `~/.config/gcloud` elsewhere).
pub fn adc_candidate_paths(env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let nonempty = |k: &str| env(k).filter(|v| !v.is_empty());
    if let Some(p) = nonempty("GOOGLE_APPLICATION_CREDENTIALS") {
        out.push(PathBuf::from(p));
    }
    let file = "application_default_credentials.json";
    if let Some(dir) = nonempty("CLOUDSDK_CONFIG") {
        out.push(PathBuf::from(dir).join(file));
    }
    if let Some(appdata) = nonempty("APPDATA") {
        out.push(PathBuf::from(appdata).join("gcloud").join(file));
    }
    if let Some(home) = nonempty("HOME").or_else(|| nonempty("USERPROFILE")) {
        out.push(
            PathBuf::from(home)
                .join(".config")
                .join("gcloud")
                .join(file),
        );
    }
    out
}

async fn build_adc(paths: &[PathBuf]) -> Result<DefaultAuthenticator, String> {
    crate::install_crypto_provider();
    for path in paths {
        let Ok(text) = tokio::fs::read_to_string(path).await else {
            continue;
        };
        let kind = adc_kind(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        match kind {
            AdcKind::ServiceAccount => {
                let key = yup_oauth2::read_service_account_key(path)
                    .await
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                return yup_oauth2::ServiceAccountAuthenticator::builder(key)
                    .build()
                    .await
                    .map_err(|e| format!("{}: {e}", path.display()));
            }
            AdcKind::AuthorizedUser => {
                let secret = yup_oauth2::read_authorized_user_secret(path)
                    .await
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                return yup_oauth2::AuthorizedUserAuthenticator::builder(secret)
                    .build()
                    .await
                    .map_err(|e| format!("{}: {e}", path.display()));
            }
            AdcKind::Other => {
                return Err(format!("{}: unsupported credential type", path.display()));
            }
        }
    }
    Err(format!(
        "application default credentials: no file found (looked in {} places; set GOOGLE_APPLICATION_CREDENTIALS)",
        paths.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_token() {
        let c = Credentials::token("ya29.super-secret");
        let shown = format!("{c:?}");
        assert!(!shown.contains("ya29"), "{shown}");
        assert!(shown.contains("Token"));
        assert!(!format!("{:?}", Secret("abc".into())).contains("abc"));
    }

    #[test]
    fn resolve_prefers_explicit_token_then_auto() {
        assert_eq!(
            Credentials::resolve(Some(" tok ".into())).source(),
            Source::Token
        );
        assert_eq!(
            Credentials::resolve(Some("  ".into())).source(),
            Source::Auto
        );
        assert_eq!(Credentials::resolve(None).source(), Source::Auto);
        assert_eq!(
            Credentials::with_source(None, Source::Adc).source(),
            Source::Adc
        );
        assert_eq!(
            Credentials::with_source(None, Source::Gcloud).source(),
            Source::Gcloud
        );
        assert_eq!(
            Credentials::with_source(Some("t".into()), Source::Adc).source(),
            Source::Token
        );
    }

    #[test]
    fn source_parse() {
        assert_eq!(Source::parse("ADC"), Some(Source::Adc));
        assert_eq!(Source::parse("gcloud"), Some(Source::Gcloud));
        assert_eq!(Source::parse("nope"), None);
    }

    #[tokio::test]
    async fn explicit_token_is_returned_for_any_scope() {
        let c = Credentials::token("abc");
        let got = c
            .get_token(&["https://www.googleapis.com/auth/drive.readonly"])
            .await
            .unwrap();
        assert_eq!(got.as_deref(), Some("abc"));
    }

    #[test]
    fn gcloud_args_carry_scopes() {
        assert_eq!(gcloud_args(&[]), ["auth", "print-access-token"]);
        let a = gcloud_args(&["s1".into(), "s2".into()]);
        assert_eq!(a, ["auth", "print-access-token", "--scopes=s1,s2"]);
    }

    #[test]
    fn gcloud_output_parsing() {
        assert_eq!(parse_gcloud_output("ya29.abc\n").unwrap(), "ya29.abc");
        assert!(parse_gcloud_output("  \n").is_err());
        assert!(parse_gcloud_output("two words").is_err());
        // an error must not echo the rejected output (it could be a token fragment)
        assert!(
            !parse_gcloud_output("ya29.a b")
                .unwrap_err()
                .contains("ya29")
        );
    }

    #[test]
    fn adc_paths_follow_the_documented_order() {
        let env = |k: &str| match k {
            "GOOGLE_APPLICATION_CREDENTIALS" => Some("/etc/sa.json".to_string()),
            "CLOUDSDK_CONFIG" => Some("/cfg".to_string()),
            "APPDATA" => Some("C:/Users/x/AppData/Roaming".to_string()),
            "HOME" => Some("/home/x".to_string()),
            _ => None,
        };
        let paths = adc_candidate_paths(&env);
        assert_eq!(paths[0], PathBuf::from("/etc/sa.json"));
        assert_eq!(
            paths[1],
            PathBuf::from("/cfg").join("application_default_credentials.json")
        );
        assert!(
            paths[2].ends_with("gcloud/application_default_credentials.json")
                || paths[2].ends_with("gcloud\\application_default_credentials.json")
        );
        assert!(paths.iter().any(|p| p.starts_with("/home/x")));
        assert!(adc_candidate_paths(&|_| None).is_empty());
    }

    #[tokio::test]
    async fn adc_failures_are_reported_without_secrets() {
        let missing = build_adc(&[]).await.err().unwrap();
        assert!(missing.contains("no file found"), "{missing}");

        let dir = tempfile::tempdir().unwrap();
        let odd = dir.path().join("odd.json");
        std::fs::write(
            &odd,
            r#"{"type":"external_account","client_secret":"hunter2"}"#,
        )
        .unwrap();
        let err = build_adc(&[dir.path().join("absent.json"), odd])
            .await
            .err()
            .unwrap();
        assert!(
            err.contains("unsupported credential type") && !err.contains("hunter2"),
            "{err}"
        );

        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, "{ not json").unwrap();
        assert!(build_adc(&[bad]).await.err().unwrap().contains("not JSON"));
    }

    #[tokio::test]
    async fn adc_file_with_a_broken_file_fails_as_an_auth_error() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("sa.json");
        std::fs::write(
            &f,
            r#"{"type":"service_account","private_key":"not a key"}"#,
        )
        .unwrap();
        let err = Credentials::adc_file(&f)
            .get_token(&["scope"])
            .await
            .err()
            .unwrap()
            .to_string();
        assert!(
            err.contains("sa.json") && !err.contains("not a key"),
            "{err}"
        );
    }

    #[test]
    fn adc_kind_from_type_field() {
        assert_eq!(
            adc_kind(r#"{"type":"service_account","private_key":"x"}"#).unwrap(),
            AdcKind::ServiceAccount
        );
        assert_eq!(
            adc_kind(r#"{"type":"authorized_user","refresh_token":"x"}"#).unwrap(),
            AdcKind::AuthorizedUser
        );
        assert_eq!(
            adc_kind(r#"{"type":"external_account"}"#).unwrap(),
            AdcKind::Other
        );
        assert!(adc_kind("not json").is_err());
    }
}
