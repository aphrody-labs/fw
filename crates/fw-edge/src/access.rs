//! Cloudflare Access JWT verification, a port of `verifyAccess` in `cloudflare/drive.ts`.
//!
//! The JWT comes from the `cf-access-jwt-assertion` header or the `CF_Authorization` cookie; the team
//! signing keys come from `${ACCESS_TEAM_DOMAIN}/cdn-cgi/access/certs` (cached per isolate for one hour).
//! RS256 verification uses the pure-Rust `rsa` + `sha2` pair so the whole token path is natively
//! unit-tested with generated keys; it costs one public-exponent modular exponentiation per request.
//!
//! [`verify`] is the entry point for the `/drive` and `/files` mounts, [`verify_jwt`] is the pure core.

use std::cell::RefCell;
use std::collections::HashMap;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rsa::sha2::Sha256;
use rsa::{BoxedUint, PublicKey};
use serde::Serialize;
use serde_json::{Map, Value};
use worker::{Env, Fetch, Headers, Request, Response, ResponseBody, Url};

const CERTS_TTL_MS: f64 = 3_600_000.0;
const NBF_LEEWAY_SECS: f64 = 60.0;

/// Who the Access token belongs to: a person (email) or an agent (service-token client id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    User,
    Service,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Identity {
    pub kind: Kind,
    /// Email for people, service-token client id (`common_name`) for agents.
    pub id: String,
}

/// One RSA signing key of the Access certs document (`/cdn-cgi/access/certs`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Jwk {
    pub kid: Option<String>,
    pub kty: Option<String>,
    pub n: Option<String>,
    pub e: Option<String>,
}

impl Jwk {
    fn from_value(v: &Value) -> Self {
        let field = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
        Self { kid: field("kid"), kty: field("kty"), n: field("n"), e: field("e") }
    }

    fn public_key(&self) -> Option<PublicKey> {
        if self.kty.as_deref().is_some_and(|k| k != "RSA") {
            return None;
        }
        let n = b64url_bytes(self.n.as_deref()?).ok()?;
        let e = b64url_bytes(self.e.as_deref()?).ok()?;
        PublicKey::new(boxed_uint(&n)?, boxed_uint(&e)?).ok()
    }
}

fn boxed_uint(be: &[u8]) -> Option<BoxedUint> {
    let bits = u32::try_from(be.len().checked_mul(8)?).ok()?.max(64).div_ceil(64) * 64;
    BoxedUint::from_be_slice(be, bits).ok()
}

/// Why a token was refused: the HTTP status and the message after the product prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reject {
    pub status: u16,
    pub message: &'static str,
}

const MALFORMED: Reject = Reject { status: 401, message: "malformed Access token" };
const UNKNOWN_KEY: Reject = Reject { status: 401, message: "unknown Access signing key" };
const BAD_SIGNATURE: Reject = Reject { status: 401, message: "invalid Access signature" };
const WRONG_APP: Reject = Reject { status: 403, message: "token was issued for another application" };
const EXPIRED: Reject = Reject { status: 401, message: "Access token expired" };
const NO_IDENTITY: Reject = Reject { status: 403, message: "token carries no identity" };

pub(crate) fn b64url_bytes(input: &str) -> Result<Vec<u8>, base64::DecodeError> {
    URL_SAFE_NO_PAD.decode(input.trim_end_matches('='))
}

fn b64url_json(input: &str) -> Option<Value> {
    serde_json::from_slice(&b64url_bytes(input).ok()?).ok()
}

/// Verifies an Access JWT against the team keys: RS256 only, kid lookup, `iss == team`, `aud` contains
/// `aud`, `exp` and `nbf` (60 s leeway), identity from `email` else `common_name`. `team` has no trailing slash.
pub fn verify_jwt(jwt: &str, keys: &[Jwk], team: &str, aud: &str, now_secs: f64) -> Result<Identity, Reject> {
    let mut parts = jwt.split('.');
    let (Some(h), Some(p), Some(s)) = (parts.next(), parts.next(), parts.next()) else { return Err(MALFORMED) };
    if h.is_empty() || p.is_empty() || s.is_empty() {
        return Err(MALFORMED);
    }
    let (Some(header), Some(claims)) = (b64url_json(h), b64url_json(p)) else { return Err(MALFORMED) };

    let kid = match header.get("kid") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => "undefined".to_owned(),
    };
    let key = keys.iter().find(|k| k.kid.as_deref() == Some(kid.as_str()));
    if header.get("alg").and_then(Value::as_str) != Some("RS256") {
        return Err(UNKNOWN_KEY);
    }
    let Some(public) = key.and_then(Jwk::public_key) else { return Err(UNKNOWN_KEY) };

    let signature = b64url_bytes(s).map_err(|_| BAD_SIGNATURE)?;
    let signed = &jwt[..h.len() + 1 + p.len()];
    public.pkcs1v15_verify::<Sha256>(signed.as_bytes(), &signature).map_err(|_| BAD_SIGNATURE)?;

    let audience_ok = match claims.get("aud") {
        Some(Value::Array(all)) => all.iter().any(|a| a.as_str() == Some(aud)),
        Some(Value::String(one)) => one == aud,
        _ => false,
    };
    if claims.get("iss").and_then(Value::as_str) != Some(team) || !audience_ok {
        return Err(WRONG_APP);
    }
    let Some(exp) = claims.get("exp").and_then(Value::as_f64) else { return Err(EXPIRED) };
    if exp < now_secs || claims.get("nbf").and_then(Value::as_f64).is_some_and(|nbf| nbf > now_secs + NBF_LEEWAY_SECS) {
        return Err(EXPIRED);
    }
    let text = |k: &str| claims.get(k).and_then(Value::as_str).filter(|v| !v.is_empty());
    if let Some(email) = text("email") {
        return Ok(Identity { kind: Kind::User, id: email.to_owned() });
    }
    if let Some(client) = text("common_name") {
        return Ok(Identity { kind: Kind::Service, id: client.to_owned() });
    }
    Err(NO_IDENTITY)
}

/// The value of cookie `name` in a `Cookie` header, as drive.ts `cookie()` reads it.
pub(crate) fn cookie_value(header: &str, name: &str) -> Option<String> {
    header.split(';').find_map(|part| {
        let (k, v) = part.trim().split_once('=').unwrap_or((part.trim(), ""));
        (k == name).then(|| v.to_owned())
    })
}

/// The JWT of a request: the Access header wins over the cookie; an empty value counts as missing.
pub(crate) fn request_jwt(header: Option<String>, cookie: Option<&str>) -> Option<String> {
    let found = header.or_else(|| cookie.and_then(|c| cookie_value(c, "CF_Authorization")));
    found.filter(|j| !j.is_empty())
}

pub(crate) fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64() * 1000.0)
    }
}

#[derive(Serialize)]
struct Problem<'a> {
    ok: bool,
    error: &'a str,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// The problem response of drive.ts `problem()`: `{ok:false,error,...extra}` as no-store JSON.
pub(crate) fn problem(status: u16, error: &str, extra: &[(&str, Value)]) -> Response {
    let body = Problem { ok: false, error, extra: extra.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect() };
    let headers = Headers::new();
    let _ = headers.set("content-type", "application/json; charset=utf-8");
    let _ = headers.set("cache-control", "no-store");
    let bytes = serde_json::to_vec(&body).unwrap_or_default();
    Response::builder().with_status(status).with_headers(headers).body(ResponseBody::Body(bytes))
}

thread_local! {
    // Isolate-wide: shared, non-request data only (the team's public keys).
    static KEYS: RefCell<HashMap<String, (Vec<Jwk>, f64)>> = RefCell::new(HashMap::new());
}

async fn team_keys(team: &str) -> Result<Vec<Jwk>, String> {
    let now = now_ms();
    if let Some(keys) = KEYS.with(|c| c.borrow().get(team).filter(|(_, exp)| *exp > now).map(|(k, _)| k.clone())) {
        return Ok(keys);
    }
    let url = Url::parse(&format!("{team}/cdn-cgi/access/certs")).map_err(|e| e.to_string())?;
    let mut res = Fetch::Url(url).send().await.map_err(|e| e.to_string())?;
    if !(200..300).contains(&res.status_code()) {
        return Err(format!("access certs: HTTP {}", res.status_code()));
    }
    let body: Value = res.json().await.map_err(|e| e.to_string())?;
    let keys: Vec<Jwk> = body.get("keys").and_then(Value::as_array).map(|a| a.iter().map(Jwk::from_value).collect()).unwrap_or_default();
    KEYS.with(|c| c.borrow_mut().insert(team.to_owned(), (keys.clone(), now_ms() + CERTS_TTL_MS)));
    Ok(keys)
}

pub(crate) fn setting(env: &Env, name: &str) -> Option<String> {
    env.var(name).ok().map(|v| v.to_string()).filter(|v| !v.is_empty())
}

/// Verifies the Access JWT of `req` for `/drive` or `/files`. `product` prefixes the error messages
/// ("drive: sign in through Cloudflare Access"). `Err` carries the 401/403/503 problem response to return.
pub async fn verify(req: &Request, env: &Env, product: &str) -> Result<Identity, Response> {
    let team = setting(env, "ACCESS_TEAM_DOMAIN").map(|t| t.trim_end_matches('/').to_owned()).filter(|t| !t.is_empty());
    let aud = setting(env, "ACCESS_AUD");
    let (Some(team), Some(aud)) = (team, aud) else {
        return Err(problem(503, &format!("{product}: Access is not configured (ACCESS_TEAM_DOMAIN, ACCESS_AUD)"), &[]));
    };
    let header = req.headers().get("cf-access-jwt-assertion").ok().flatten();
    let cookie = req.headers().get("cookie").ok().flatten();
    let Some(jwt) = request_jwt(header, cookie.as_deref()) else {
        return Err(problem(401, &format!("{product}: sign in through Cloudflare Access"), &[("login", Value::String(team))]));
    };
    // Keys are only fetched for tokens that parse, like drive.ts.
    let parsed = jwt.split('.').take(3).filter(|s| !s.is_empty()).count() == 3;
    let keys = if parsed {
        team_keys(&team).await.map_err(|e| problem(502, &format!("{product}: {e}"), &[]))?
    } else {
        Vec::new()
    };
    verify_jwt(&jwt, &keys, &team, &aud, now_ms() / 1000.0).map_err(|r| problem(r.status, &format!("{product}: {}", r.message), &[]))
}

/// Generated RSA keys shared by the unit tests of this crate (2048-bit keygen is slow, so once per process).
#[cfg(test)]
pub(crate) mod testutil {
    use std::sync::OnceLock;

    static KEYS: [OnceLock<rsa::PrivateKey>; 2] = [OnceLock::new(), OnceLock::new()];

    pub(crate) fn keys(i: usize) -> &'static rsa::PrivateKey {
        KEYS[i].get_or_init(|| rsa::PrivateKey::new(&mut rand::rng(), 2048).expect("keygen"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::sha2::Sha256;
    use rsa::traits::PublicKeyParts;

    pub(crate) const TEAM: &str = "https://team.cloudflareaccess.com";
    pub(crate) const AUD: &str = "aud-drive";
    const NOW: f64 = 1_800_000_000.0;

    fn jwk(kid: &str, key: &rsa::PrivateKey) -> Jwk {
        let public = key.to_public_key();
        Jwk {
            kid: Some(kid.to_owned()),
            kty: Some("RSA".to_owned()),
            n: Some(URL_SAFE_NO_PAD.encode(public.n_bytes())),
            e: Some(URL_SAFE_NO_PAD.encode(public.e_bytes())),
        }
    }

    fn b64(v: &Value) -> String {
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(v).unwrap())
    }

    fn token(key: &rsa::PrivateKey, header: Value, claims: Value) -> String {
        let signed = format!("{}.{}", b64(&header), b64(&claims));
        let sig = key.pkcs1v15_sign::<Sha256>(signed.as_bytes()).unwrap();
        format!("{signed}.{}", URL_SAFE_NO_PAD.encode(sig))
    }

    fn user(exp_offset: f64) -> Value {
        serde_json::json!({ "iss": TEAM, "aud": [AUD], "exp": NOW + exp_offset, "email": "person@example.com" })
    }

    fn check(jwt: &str, keys: &[Jwk]) -> Result<Identity, Reject> {
        verify_jwt(jwt, keys, TEAM, AUD, NOW)
    }

    #[test]
    fn jwt_matrix() {
        let (good, other) = (testutil::keys(0), testutil::keys(1));
        let keys = [jwk("k1", good)];
        let rs = serde_json::json!({ "alg": "RS256", "kid": "k1" });

        // people carry an email, agents a service-token client id
        let ok = check(&token(good, rs.clone(), user(600.0)), &keys).unwrap();
        assert_eq!(ok, Identity { kind: Kind::User, id: "person@example.com".into() });
        let svc = serde_json::json!({ "iss": TEAM, "aud": AUD, "exp": NOW + 60.0, "common_name": "abc.access" });
        let ok = check(&token(good, rs.clone(), svc), &keys).unwrap();
        assert_eq!(ok, Identity { kind: Kind::Service, id: "abc.access".into() });
        let none = serde_json::json!({ "iss": TEAM, "aud": [AUD], "exp": NOW + 60.0 });
        assert_eq!(check(&token(good, rs.clone(), none), &keys), Err(NO_IDENTITY));

        // malformed
        assert_eq!(check("", &keys), Err(MALFORMED));
        assert_eq!(check("a.b", &keys), Err(MALFORMED));
        assert_eq!(check("a..c", &keys), Err(MALFORMED));
        assert_eq!(check("!!!.@@@.###", &keys), Err(MALFORMED));

        // foreign key: signed by another key under a known kid, and an unknown kid, and a non-RS256 alg
        assert_eq!(check(&token(other, rs.clone(), user(600.0)), &keys), Err(BAD_SIGNATURE));
        let unknown = serde_json::json!({ "alg": "RS256", "kid": "nope" });
        assert_eq!(check(&token(good, unknown, user(600.0)), &keys), Err(UNKNOWN_KEY));
        let hs = serde_json::json!({ "alg": "HS256", "kid": "k1" });
        assert_eq!(check(&token(good, hs, user(600.0)), &keys), Err(UNKNOWN_KEY));
        assert_eq!(check(&token(good, rs.clone(), user(600.0)), &[]), Err(UNKNOWN_KEY));

        // tampered payload keeps the header and signature of another token
        let t = token(good, rs.clone(), user(600.0));
        let forged = token(good, rs.clone(), user(900.0));
        let mixed = format!("{}.{}.{}", t.split('.').next().unwrap(), forged.split('.').nth(1).unwrap(), t.split('.').nth(2).unwrap());
        assert_eq!(check(&mixed, &keys), Err(BAD_SIGNATURE));

        // wrong application: audience, string audience, issuer
        let wrong_aud = serde_json::json!({ "iss": TEAM, "aud": ["other"], "exp": NOW + 60.0, "email": "a@b.c" });
        assert_eq!(check(&token(good, rs.clone(), wrong_aud), &keys), Err(WRONG_APP));
        let wrong_iss = serde_json::json!({ "iss": "https://evil.example", "aud": [AUD], "exp": NOW + 60.0, "email": "a@b.c" });
        assert_eq!(check(&token(good, rs.clone(), wrong_iss), &keys), Err(WRONG_APP));
        let no_aud = serde_json::json!({ "iss": TEAM, "exp": NOW + 60.0, "email": "a@b.c" });
        assert_eq!(check(&token(good, rs.clone(), no_aud), &keys), Err(WRONG_APP));

        // exp and nbf (60 s leeway)
        assert_eq!(check(&token(good, rs.clone(), user(-1.0)), &keys), Err(EXPIRED));
        let no_exp = serde_json::json!({ "iss": TEAM, "aud": [AUD], "email": "a@b.c" });
        assert_eq!(check(&token(good, rs.clone(), no_exp), &keys), Err(EXPIRED));
        let mut nbf = user(600.0);
        nbf["nbf"] = Value::from(NOW + 61.0);
        assert_eq!(check(&token(good, rs.clone(), nbf), &keys), Err(EXPIRED));
        let mut nbf = user(600.0);
        nbf["nbf"] = Value::from(NOW + 60.0);
        assert!(check(&token(good, rs, nbf), &keys).is_ok());
    }

    #[test]
    fn jwk_without_rsa_material_never_verifies() {
        let good = testutil::keys(0);
        let rs = serde_json::json!({ "alg": "RS256", "kid": "k1" });
        let t = token(good, rs, user(600.0));
        let ec = Jwk { kid: Some("k1".into()), kty: Some("EC".into()), n: None, e: None };
        assert_eq!(check(&t, &[ec]), Err(UNKNOWN_KEY));
        let empty = Jwk { kid: Some("k1".into()), kty: Some("RSA".into()), n: None, e: None };
        assert_eq!(check(&t, &[empty]), Err(UNKNOWN_KEY));
    }

    #[test]
    fn jwt_source_header_then_cookie() {
        assert_eq!(cookie_value("a=b; CF_Authorization=x.y.z; c=d", "CF_Authorization").as_deref(), Some("x.y.z"));
        assert_eq!(cookie_value("CF_Authorization=a=b", "CF_Authorization").as_deref(), Some("a=b"));
        assert_eq!(cookie_value("a=b", "CF_Authorization"), None);
        assert_eq!(request_jwt(Some("h".into()), Some("CF_Authorization=c")).as_deref(), Some("h"));
        assert_eq!(request_jwt(None, Some("CF_Authorization=c")).as_deref(), Some("c"));
        assert_eq!(request_jwt(Some(String::new()), Some("CF_Authorization=c")), None);
        assert_eq!(request_jwt(None, None), None);
    }

    #[test]
    fn jwk_parsing_is_lenient() {
        let v: Value = serde_json::json!({ "kid": 7, "kty": "RSA", "n": "AQAB", "alg": "RS256" });
        let j = Jwk::from_value(&v);
        assert_eq!((j.kid, j.kty.as_deref(), j.n.as_deref(), j.e), (None, Some("RSA"), Some("AQAB"), None));
    }
}
