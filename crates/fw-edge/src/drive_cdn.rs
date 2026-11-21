//! The Drive CDN mounted at `/drive`, a port of `cloudflare/drive.ts`: Cloudflare Access (see [`crate::access`])
//! -> R2 cache `DRIVE_CACHE` keyed by file version -> Google Drive v3 read through a service account that the
//! root folder is shared with. Folders list as JSON or HTML, files stream with Range, ETag and 304 support,
//! Google-native files export to an open format.
//!
//! Tests can stand in for Google: `GOOGLE_API_BASE` (default `https://www.googleapis.com`) and
//! `GOOGLE_TOKEN_URI` (default: the service-account key's `token_uri`) override the two origins.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write as _;

use futures_util::future::join;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::ReadableStream;
use worker::worker_sys::FixedLengthStream;
use worker::{
    Bucket, Conditional, Context, Data, Date, DateInit, Env, Fetch, Headers, HttpMetadata, Method, Object, Range, Request,
    RequestInit, Response, ResponseBody, Result, Url, console_error,
};

use crate::access::{self, Identity, now_ms, problem, setting};

const PREFIX: &str = "/drive";
const FOLDER: &str = "application/vnd.google-apps.folder";
const SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";
const DEFAULT_API_BASE: &str = "https://www.googleapis.com";
const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const FILE_FIELDS: &str = "id,name,mimeType,size,md5Checksum,modifiedTime,parents";
const META_TTL_MS: f64 = 60_000.0;
const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;
const CSP: &str = "sandbox; default-src 'none'; img-src data: blob:; media-src data: blob:; style-src 'unsafe-inline'";

/// Default export target per Google-native type; `?export=<mime>` overrides.
pub(crate) fn export_default(mime: &str) -> Option<(&'static str, &'static str)> {
    Some(match mime {
        "application/vnd.google-apps.document" => ("text/markdown", "md"),
        "application/vnd.google-apps.spreadsheet" => ("text/csv", "csv"),
        "application/vnd.google-apps.presentation" => ("application/pdf", "pdf"),
        "application/vnd.google-apps.drawing" => ("image/svg+xml", "svg"),
        "application/vnd.google-apps.script" => ("application/vnd.google-apps.script+json", "json"),
        _ => return None,
    })
}

thread_local! {
    // Isolate-wide caches hold only shared, non-request data: the service-account token and short-lived
    // Drive metadata identical for every authorised caller.
    static TOKENS: RefCell<HashMap<String, (String, f64)>> = RefCell::new(HashMap::new());
    static META: RefCell<HashMap<String, (String, f64)>> = RefCell::new(HashMap::new());
}

#[derive(Debug)]
enum DriveErr {
    Config(String),
    NotFound,
    Other(String),
}

impl From<worker::Error> for DriveErr {
    fn from(e: worker::Error) -> Self {
        Self::Other(e.to_string())
    }
}

impl From<JsValue> for DriveErr {
    fn from(e: JsValue) -> Self {
        Self::Other(e.as_string().or_else(|| js_sys::Reflect::get(&e, &"message".into()).ok().and_then(|m| m.as_string())).unwrap_or_else(|| format!("{e:?}")))
    }
}

type DriveResult<T> = std::result::Result<T, DriveErr>;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveFile {
    id: String,
    name: String,
    mime_type: String,
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    md5_checksum: Option<String>,
    #[serde(default)]
    modified_time: Option<String>,
    #[serde(default)]
    parents: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct DriveList {
    #[serde(default)]
    files: Vec<DriveFile>,
    #[serde(default, rename = "nextPageToken")]
    next_page_token: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Pure helpers (natively unit-tested)
// ---------------------------------------------------------------------------------------------

/// `encodeURIComponent`.
pub(crate) fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => out.push(char::from(b)),
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// `decodeURIComponent`; malformed input is the same "URI malformed" error JavaScript throws.
pub(crate) fn decode_uri_component(s: &str) -> std::result::Result<String, &'static str> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
            out.push(hex.ok_or("URI malformed")?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "URI malformed")
}

/// Drive query string literal: `'` and `\` escaped.
pub(crate) fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn drive_query(q: &str) -> String {
    let fields = format!("nextPageToken,files({FILE_FIELDS})");
    format!("/files?q={}&fields={}&pageSize=1000&orderBy=folder,name&supportsAllDrives=true&includeItemsFromAllDrives=true", encode_uri_component(q), encode_uri_component(&fields))
}

fn file_path(id: &str) -> String {
    format!("/files/{}?fields={FILE_FIELDS}&supportsAllDrives=true", encode_uri_component(id))
}

fn disposition(name: &str) -> String {
    format!("inline; filename*=UTF-8''{}", encode_uri_component(name))
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// `formatBytes` of drive.ts: one decimal above bytes, ties rounded up like `Number.prototype.toFixed`.
pub(crate) fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut i = 0;
    let mut unit = 1u128;
    while i < UNITS.len() - 1 && u128::from(n) >= unit * 1024 {
        unit *= 1024;
        i += 1;
    }
    if i == 0 {
        return format!("{n} B");
    }
    let tenths = (u128::from(n) * 20 + unit) / (2 * unit);
    format!("{}.{} {}", tenths / 10, tenths % 10, UNITS[i])
}

#[derive(Debug, Serialize)]
struct Entry {
    name: String,
    kind: &'static str,
    #[serde(rename = "mimeType")]
    mime_type: String,
    size: Option<u64>,
    modified: Option<String>,
    id: String,
    href: String,
    export: Option<&'static str>,
}

fn entries(base: &str, children: &[DriveFile]) -> Vec<Entry> {
    children
        .iter()
        .map(|f| {
            let dir = f.mime_type == FOLDER;
            Entry {
                name: f.name.clone(),
                kind: if dir { "dir" } else { "file" },
                mime_type: f.mime_type.clone(),
                size: f.size.as_deref().filter(|s| !s.is_empty()).and_then(|s| s.parse().ok()),
                modified: f.modified_time.clone(),
                id: f.id.clone(),
                href: format!("{base}{}{}", encode_uri_component(&f.name), if dir { "/" } else { "" }),
                export: export_default(&f.mime_type).map(|(mime, _)| mime),
            }
        })
        .collect()
}

fn listing_base(path: &str) -> String {
    if path.is_empty() {
        format!("{PREFIX}/")
    } else {
        format!("{PREFIX}/{}/", path.split('/').map(encode_uri_component).collect::<Vec<_>>().join("/"))
    }
}

#[derive(Serialize)]
struct ListingJson<'a> {
    ok: bool,
    path: String,
    id: &'a str,
    name: &'a str,
    viewer: &'a Identity,
    entries: &'a [Entry],
}

fn listing_json(folder: &DriveFile, path: &str, who: &Identity, entries: &[Entry]) -> String {
    let body = ListingJson { ok: true, path: format!("/{path}"), id: &folder.id, name: &folder.name, viewer: who, entries };
    serde_json::to_string(&body).unwrap_or_default()
}

fn listing_html(folder: &DriveFile, path: &str, who: &Identity, entries: &[Entry]) -> String {
    let rows: String = entries
        .iter()
        .map(|e| {
            let right = if e.kind == "dir" {
                "Folder".to_owned()
            } else if let Some(size) = e.size {
                format_bytes(size)
            } else {
                escape_html(e.export.unwrap_or(&e.mime_type))
            };
            format!("<li><a href=\"{}\">{}{}</a><span>{right}</span></li>", escape_html(&e.href), escape_html(&e.name), if e.kind == "dir" { "/" } else { "" })
        })
        .collect();
    let mut crumbs = vec![format!("<a href=\"{PREFIX}/\">Drive</a>")];
    if !path.is_empty() {
        let segments: Vec<&str> = path.split('/').collect();
        for (i, seg) in segments.iter().enumerate() {
            let href = segments[..=i].iter().map(|s| encode_uri_component(s)).collect::<Vec<_>>().join("/");
            crumbs.push(format!("<a href=\"{PREFIX}/{href}/\">{}</a>", escape_html(seg)));
        }
    }
    let crumbs = crumbs.join(" / ");
    let name = escape_html(&folder.name);
    let list = if entries.is_empty() { "<p>This folder is empty. Files added in Google Drive appear here within a minute.</p>".to_owned() } else { format!("<ul>{rows}</ul>") };
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{name}: fw Drive</title><link rel=\"stylesheet\" href=\"/app.css\"><style>
main{{max-width:56rem;margin:0 auto;padding:24px 16px;font-family:var(--font);color:var(--md-sys-color-on-surface)}}
body{{background:var(--md-sys-color-surface)}}nav{{color:var(--md-sys-color-on-surface-variant);margin-bottom:16px}}
ul{{list-style:none;padding:0;margin:0;border-radius:16px;overflow:hidden;background:var(--md-sys-color-surface-container)}}
li{{display:flex;justify-content:space-between;gap:16px;min-height:48px;align-items:center;padding:0 16px;border-bottom:1px solid var(--md-sys-color-outline-variant)}}
li:last-child{{border-bottom:0}}a{{color:var(--md-sys-color-primary)}}a:focus-visible{{outline:3px solid var(--md-sys-color-primary);outline-offset:2px}}
li span{{color:var(--md-sys-color-on-surface-variant);font-family:var(--font-code);font-size:14px;white-space:nowrap}}p{{color:var(--md-sys-color-on-surface-variant)}}
</style></head><body><main><nav>{crumbs}</nav><h1>{name}</h1>{list}<p>Signed in as {}. Add <code>?format=json</code> for the machine listing.</p></main></body></html>",
        escape_html(&who.id)
    )
}

pub(crate) fn wants_html(accept: &str, format: Option<&str>) -> bool {
    accept.contains("text/html") && format != Some("json")
}

/// A single `Range: bytes=...` as R2 reads it; multi-range, other units and garbage are ignored (full body).
pub(crate) fn parse_range(header: &str) -> Option<Range> {
    let header = header.trim();
    let spec = header.get(..6).filter(|u| u.eq_ignore_ascii_case("bytes=")).map(|_| header[6..].trim())?;
    if spec.contains(',') {
        return None;
    }
    let (first, last) = spec.split_once('-')?;
    let (first, last) = (first.trim(), last.trim());
    let digits = |s: &str| -> Option<u64> { (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse::<u64>().ok()).flatten() };
    match (first.is_empty(), last.is_empty()) {
        (true, true) => None,
        (true, false) => Some(Range::Suffix { suffix: digits(last)?.min(MAX_SAFE_INTEGER) }),
        (false, true) => Some(Range::OffsetToEnd { offset: Some(digits(first)?).filter(|o| *o <= MAX_SAFE_INTEGER)? }),
        (false, false) => {
            let (start, end) = (digits(first)?, digits(last)?);
            if start > MAX_SAFE_INTEGER || end < start {
                return None;
            }
            if end > MAX_SAFE_INTEGER {
                return Some(Range::OffsetToEnd { offset: start });
            }
            Some(Range::OffsetWithLength { offset: start, length: end - start + 1 })
        }
    }
}

/// `(offset, length)` of the bytes a ranged R2 read covers.
pub(crate) fn range_window(range: &Range, size: u64) -> (u64, u64) {
    match *range {
        Range::OffsetWithLength { offset, length } => (offset, length),
        Range::OffsetToEnd { offset } => (offset, size.saturating_sub(offset)),
        Range::Prefix { length } => (0, length.min(size)),
        Range::Suffix { suffix } => (size.saturating_sub(suffix), suffix.min(size)),
    }
}

fn etag_values(header: &str) -> Vec<String> {
    header.split(',').map(|t| t.trim().trim_start_matches("W/").trim_matches('"').to_owned()).filter(|t| !t.is_empty()).collect()
}

/// The etag of an `If-Match` / `If-None-Match` header holding exactly one tag: what R2's `etagMatches` takes.
fn single_etag(header: &str) -> Option<String> {
    let mut all = etag_values(header);
    (all.len() == 1 && all[0] != "*").then(|| all.remove(0))
}

/// R2 conditions that [`Conditional`] cannot hold (several tags, `*`), checked here against the object's etag.
/// `true` when the request's preconditions fail for this etag.
pub(crate) fn etag_precondition_fails(if_match: Option<&str>, if_none_match: Option<&str>, etag: &str) -> bool {
    if let Some(h) = if_match {
        let all = etag_values(h);
        if all.len() > 1 && !all.iter().any(|t| t == "*" || t == etag) {
            return true;
        }
    }
    if let Some(h) = if_none_match {
        let all = etag_values(h);
        if all.iter().any(|t| t == "*") || (all.len() > 1 && all.iter().any(|t| t == etag)) {
            return true;
        }
    }
    false
}

fn http_date_ms(value: &str) -> Option<u64> {
    let since = httpdate::parse_http_date(value.trim()).ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
    u64::try_from(since.as_millis()).ok()
}

/// The request's conditional headers as R2 conditions (RFC 7232 precedence). R2 compares upload times in
/// milliseconds, HTTP dates are whole seconds: `If-Modified-Since: D` means "uploaded after the end of
/// second D", `If-Unmodified-Since: D` means "uploaded before the end of second D".
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Preconditions {
    pub etag_matches: Option<String>,
    pub etag_does_not_match: Option<String>,
    pub uploaded_after_ms: Option<u64>,
    pub uploaded_before_ms: Option<u64>,
}

impl Preconditions {
    pub(crate) fn from_headers(get: impl Fn(&str) -> Option<String>) -> Self {
        let mut p = Self::default();
        if let Some(m) = get("if-match") {
            p.etag_matches = single_etag(&m);
        } else if let Some(d) = get("if-unmodified-since") {
            p.uploaded_before_ms = http_date_ms(&d).map(|ms| ms + 1000);
        }
        if let Some(m) = get("if-none-match") {
            p.etag_does_not_match = single_etag(&m);
        } else if let Some(d) = get("if-modified-since") {
            p.uploaded_after_ms = http_date_ms(&d).map(|ms| ms + 999);
        }
        p
    }

    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn into_conditional(self) -> Conditional {
        let date = |ms: u64| Date::new(DateInit::Millis(ms));
        Conditional {
            etag_matches: self.etag_matches,
            etag_does_not_match: self.etag_does_not_match,
            uploaded_after: self.uploaded_after_ms.map(date),
            uploaded_before: self.uploaded_before_ms.map(date),
        }
    }
}

/// Status and framing of a response served from R2 (`fromR2` in drive.ts).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    pub status: u16,
    pub content_range: Option<String>,
    pub content_length: Option<u64>,
    pub body: bool,
}

pub(crate) fn plan(head: bool, has_body: bool, conditional_header: bool, ranged: Option<(u64, u64)>, size: u64) -> Plan {
    if !has_body {
        return Plan { status: if conditional_header { 304 } else { 412 }, content_range: None, content_length: None, body: false };
    }
    match ranged {
        Some((offset, length)) => Plan {
            status: 206,
            content_range: Some(format!("bytes {offset}-{}/{size}", (offset + length).saturating_sub(1))),
            content_length: Some(length),
            body: !head,
        },
        None => Plan { status: 200, content_range: None, content_length: Some(size), body: !head },
    }
}

fn file_headers(content_type: Option<&str>, content_disposition: Option<&str>, etag: &str, uploaded_ms: Option<u64>, cache: &str) -> Result<Headers> {
    let h = Headers::new();
    if let Some(v) = content_type.filter(|v| !v.is_empty()) {
        h.set("content-type", v)?;
    }
    if let Some(v) = content_disposition.filter(|v| !v.is_empty()) {
        h.set("content-disposition", v)?;
    }
    h.set("etag", etag)?;
    if let Some(ms) = uploaded_ms {
        h.set("last-modified", &httpdate::fmt_http_date(std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms)))?;
    }
    h.set("accept-ranges", "bytes")?;
    h.set("cache-control", "private, no-cache")?;
    h.set("x-content-type-options", "nosniff")?;
    // Drive content is untrusted: an HTML or SVG file must not run script on the fw.aphrody.com origin.
    h.set("content-security-policy", CSP)?;
    h.set("x-fw-drive-cache", cache)?;
    Ok(h)
}

/// The unsigned `header.claims` of the service-account JWT-bearer grant (RFC 7523).
pub(crate) fn bearer_assertion(client_email: &str, token_uri: &str, iat: u64) -> String {
    #[derive(Serialize)]
    struct Claims<'a> {
        iss: &'a str,
        scope: &'a str,
        aud: &'a str,
        iat: u64,
        exp: u64,
    }
    let enc = |bytes: &[u8]| base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes);
    let claims = Claims { iss: client_email, scope: SCOPE, aud: token_uri, iat, exp: iat + 3600 };
    format!("{}.{}", enc(br#"{"alg":"RS256","typ":"JWT"}"#), enc(&serde_json::to_vec(&claims).unwrap_or_default()))
}

/// PEM `PRIVATE KEY` block to PKCS#8 DER.
pub(crate) fn pem_to_pkcs8(pem: &str) -> std::result::Result<Vec<u8>, String> {
    let body: String = pem.lines().map(str::trim).filter(|l| !l.starts_with("-----")).collect();
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, body.split_whitespace().collect::<String>()).map_err(|e| format!("GOOGLE_SA_KEY private_key: {e}"))
}

// RS256 sign: WebCrypto runs in the host at native speed, which keeps a cold isolate's CPU time near zero;
// native builds (unit tests) use the pure-Rust `rsa` path, which also checks the PKCS#1 v1.5 framing.
#[cfg(target_arch = "wasm32")]
async fn sign_rs256(pkcs8: &[u8], message: &[u8]) -> std::result::Result<Vec<u8>, String> {
    use js_sys::{Array, Object, Reflect, Uint8Array};
    let js = |e: JsValue| format!("{e:?}");
    let crypto: web_sys::Crypto = Reflect::get(&js_sys::global(), &"crypto".into()).map_err(js)?.unchecked_into();
    let subtle = crypto.subtle();
    let algorithm = Object::new();
    Reflect::set(&algorithm, &"name".into(), &"RSASSA-PKCS1-v1_5".into()).map_err(js)?;
    Reflect::set(&algorithm, &"hash".into(), &"SHA-256".into()).map_err(js)?;
    let usages = Array::of1(&"sign".into());
    let der = Uint8Array::from(pkcs8);
    let key = JsFuture::from(subtle.import_key_with_object("pkcs8", &der, &algorithm, false, &usages).map_err(js)?).await.map_err(js)?;
    let key: web_sys::CryptoKey = key.unchecked_into();
    let sig = JsFuture::from(subtle.sign_with_object_and_u8_array(&algorithm, &key, message).map_err(js)?).await.map_err(js)?;
    Ok(Uint8Array::new(&sig).to_vec())
}

#[cfg(not(target_arch = "wasm32"))]
async fn sign_rs256(pkcs8: &[u8], message: &[u8]) -> std::result::Result<Vec<u8>, String> {
    use rsa::pkcs8::DecodePrivateKey;
    let key = rsa::PrivateKey::from_pkcs8_der(pkcs8).map_err(|e| e.to_string())?;
    key.pkcs1v15_sign::<rsa::sha2::Sha256>(message).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------------
// Google Drive client
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct ServiceAccount {
    client_email: String,
    private_key: String,
    #[serde(default)]
    token_uri: Option<String>,
}

#[derive(Deserialize)]
struct TokenGrant {
    access_token: String,
    expires_in: f64,
}

struct Drive {
    api: String,
    sa_key: Option<String>,
    token_uri: Option<String>,
    root: String,
}

async fn fetch_raw(url: &str, method: Method, headers: &[(&str, &str)], body: Option<String>) -> DriveResult<Response> {
    let h = Headers::new();
    for (k, v) in headers {
        h.set(k, v)?;
    }
    let mut init = RequestInit::new();
    init.with_method(method).with_headers(h);
    if let Some(b) = body {
        init.with_body(Some(JsValue::from_str(&b)));
    }
    let request = Request::new_with_init(url, &init)?;
    Ok(Fetch::Request(request).send().await?)
}

fn ok(status: u16) -> bool {
    (200..300).contains(&status)
}

impl Drive {
    fn from_env(env: &Env, root: String) -> Self {
        let base = setting(env, "GOOGLE_API_BASE").unwrap_or_else(|| DEFAULT_API_BASE.to_owned());
        Self {
            api: format!("{}/drive/v3", base.trim_end_matches('/')),
            sa_key: env.secret("GOOGLE_SA_KEY").ok().map(|s| s.to_string()).filter(|s| !s.is_empty()),
            token_uri: setting(env, "GOOGLE_TOKEN_URI"),
            root,
        }
    }

    /// OAuth 2.0 JWT-bearer grant for the service account, cached until five minutes before expiry.
    async fn token(&self) -> DriveResult<String> {
        let Some(raw) = &self.sa_key else { return Err(DriveErr::Config("drive: GOOGLE_SA_KEY secret is not set".into())) };
        let sa: ServiceAccount = serde_json::from_str(raw).map_err(|e| DriveErr::Other(format!("GOOGLE_SA_KEY: {e}")))?;
        if let Some(token) = TOKENS.with(|c| c.borrow().get(&sa.client_email).filter(|(_, exp)| *exp > now_ms()).map(|(t, _)| t.clone())) {
            return Ok(token);
        }
        let token_uri = self.token_uri.clone().or(sa.token_uri.clone()).unwrap_or_else(|| DEFAULT_TOKEN_URI.to_owned());
        let iat = (now_ms() / 1000.0).floor() as u64;
        let unsigned = bearer_assertion(&sa.client_email, &token_uri, iat);
        let der = pem_to_pkcs8(&sa.private_key).map_err(DriveErr::Other)?;
        let signature = sign_rs256(&der, unsigned.as_bytes()).await.map_err(DriveErr::Other)?;
        let assertion = format!("{unsigned}.{}", base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, signature));
        let form = format!("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={}", encode_uri_component(&assertion));
        let mut res = fetch_raw(&token_uri, Method::Post, &[("content-type", "application/x-www-form-urlencoded")], Some(form)).await?;
        if !ok(res.status_code()) {
            return Err(DriveErr::Other(format!("google token: HTTP {} {}", res.status_code(), res.text().await.unwrap_or_default())));
        }
        let grant: TokenGrant = res.json().await?;
        let expires = now_ms() + (grant.expires_in - 300.0) * 1000.0;
        TOKENS.with(|c| c.borrow_mut().insert(sa.client_email, (grant.access_token.clone(), expires)));
        Ok(grant.access_token)
    }

    async fn json<T: DeserializeOwned>(&self, path: &str) -> DriveResult<T> {
        let now = now_ms();
        if let Some(text) = META.with(|c| c.borrow().get(path).filter(|(_, exp)| *exp > now).map(|(t, _)| t.clone())) {
            return serde_json::from_str(&text).map_err(|e| DriveErr::Other(e.to_string()));
        }
        let auth = format!("Bearer {}", self.token().await?);
        let mut res = fetch_raw(&format!("{}{path}", self.api), Method::Get, &[("authorization", &auth)], None).await?;
        if res.status_code() == 404 {
            return Err(DriveErr::NotFound);
        }
        if !ok(res.status_code()) {
            let status = res.status_code();
            let name = path.split('?').next().unwrap_or(path);
            return Err(DriveErr::Other(format!("drive {name}: HTTP {status} {}", res.text().await.unwrap_or_default())));
        }
        let text = res.text().await?;
        let value = serde_json::from_str(&text).map_err(|e| DriveErr::Other(e.to_string()))?;
        META.with(|c| {
            let mut cache = c.borrow_mut();
            if cache.len() > 512 {
                cache.retain(|_, (_, exp)| *exp > now);
            }
            cache.insert(path.to_owned(), (text, now_ms() + META_TTL_MS));
        });
        Ok(value)
    }

    async fn file_meta(&self, id: &str) -> DriveResult<DriveFile> {
        self.json(&file_path(id)).await
    }

    async fn list_children(&self, id: &str) -> DriveResult<Vec<DriveFile>> {
        let mut out = Vec::new();
        let mut token = String::new();
        loop {
            let mut path = drive_query(&format!("{} in parents and trashed=false", quote(id)));
            if !token.is_empty() {
                let _ = write!(path, "&pageToken={}", encode_uri_component(&token));
            }
            let page: DriveList = self.json(&path).await?;
            out.extend(page.files);
            token = page.next_page_token.unwrap_or_default();
            if token.is_empty() {
                return Ok(out);
            }
        }
    }

    async fn resolve_path(&self, segments: &[String]) -> DriveResult<DriveFile> {
        let mut current = self.file_meta(&self.root).await?;
        for name in segments {
            if current.mime_type != FOLDER {
                return Err(DriveErr::NotFound);
            }
            let page: DriveList = self.json(&drive_query(&format!("{} in parents and name={} and trashed=false", quote(&current.id), quote(name)))).await?;
            current = page.files.into_iter().next().ok_or(DriveErr::NotFound)?;
        }
        Ok(current)
    }

    /// Ids reach only files under the root folder, never every file the service account could read.
    async fn under_root(&self, file: &DriveFile) -> DriveResult<bool> {
        let mut node = file.clone();
        for _ in 0..32 {
            if node.id == self.root {
                return Ok(true);
            }
            let Some(parent) = node.parents.as_ref().and_then(|p| p.first()).cloned() else { return Ok(false) };
            node = self.file_meta(&parent).await?;
        }
        Ok(false)
    }
}

// ---------------------------------------------------------------------------------------------
// R2 cache
// ---------------------------------------------------------------------------------------------

#[derive(Clone)]
struct Meta {
    content_type: String,
    disposition: String,
    drive_id: String,
    name: String,
    version: String,
}

impl Meta {
    fn http(&self) -> HttpMetadata {
        HttpMetadata { content_type: Some(self.content_type.clone()), content_disposition: Some(self.disposition.clone()), ..HttpMetadata::default() }
    }

    fn custom(&self) -> HashMap<String, String> {
        HashMap::from([("driveId".to_owned(), self.drive_id.clone()), ("name".to_owned(), self.name.clone()), ("version".to_owned(), self.version.clone())])
    }
}

enum Lookup {
    Miss,
    Hit { object: Object, precondition_failed: bool },
    Unsatisfiable(u64),
}

fn header(req: &Request, name: &str) -> Option<String> {
    req.headers().get(name).ok().flatten()
}

/// `bucket.get(key)` with the request's Range and conditional headers translated for R2.
async fn cache_get(bucket: &Bucket, key: &str, req: &Request) -> DriveResult<Lookup> {
    let range = header(req, "range").as_deref().and_then(parse_range);
    let conditions = Preconditions::from_headers(|n| header(req, n));
    let mut get = bucket.get(key);
    if let Some(r) = range.clone() {
        get = get.range(r);
    }
    if !conditions.is_empty() {
        get = get.only_if(conditions.into_conditional());
    }
    let object = match get.execute().await {
        Ok(Some(o)) => o,
        Ok(None) => return Ok(Lookup::Miss),
        // R2 refuses a range that starts past the end: answer 416 for a cached object, miss otherwise.
        Err(e) if range.is_some() => {
            return match bucket.head(key).await {
                Ok(Some(o)) => Ok(Lookup::Unsatisfiable(o.size())),
                Ok(None) => Ok(Lookup::Miss),
                Err(_) => Err(e.into()),
            };
        }
        Err(e) => return Err(e.into()),
    };
    let precondition_failed = etag_precondition_fails(header(req, "if-match").as_deref(), header(req, "if-none-match").as_deref(), &object.etag());
    Ok(Lookup::Hit { object, precondition_failed })
}

fn from_r2(req: &Request, object: &Object, body_allowed: bool, cache: &str) -> DriveResult<Response> {
    let meta = object.http_metadata();
    let headers = file_headers(meta.content_type.as_deref(), meta.content_disposition.as_deref(), &object.http_etag(), Some(object.uploaded().as_millis()), cache)?;
    let has_body = body_allowed && object.body().is_some();
    let size = object.size();
    // Only a range R2 was asked for is a 206: unreadable or multi-range headers get the whole object.
    let asked = header(req, "range").as_deref().and_then(parse_range).is_some();
    let ranged = if asked { object.range().ok().map(|r| range_window(&r, size)) } else { None };
    let conditional = req.headers().has("if-none-match").unwrap_or(false) || req.headers().has("if-modified-since").unwrap_or(false);
    let p = plan(req.method() == Method::Head, has_body, conditional, ranged, size);
    if let Some(range) = &p.content_range {
        headers.set("content-range", range)?;
    }
    if let Some(len) = p.content_length {
        headers.set("content-length", &len.to_string())?;
    }
    let body = match (p.body, object.body()) {
        (true, Some(b)) => b.response_body()?,
        _ => ResponseBody::Empty,
    };
    Ok(Response::builder().with_status(p.status).with_headers(headers).body(body))
}

fn range_not_satisfiable(size: u64) -> DriveResult<Response> {
    let headers = Headers::new();
    headers.set("content-range", &format!("bytes */{size}"))?;
    headers.set("cache-control", "private, no-cache")?;
    Ok(Response::builder().with_status(416).with_headers(headers).empty())
}

/// Streams `body` (exactly `size` bytes) into R2. R2 needs the length up front; `FixedLengthStream` supplies it
/// and the bytes move host-side, never through wasm memory.
async fn store_stream(bucket: &Bucket, key: &str, body: ReadableStream, size: u64, meta: &Meta) -> DriveResult<()> {
    let fixed = match u32::try_from(size) {
        Ok(n) => FixedLengthStream::new(n)?,
        Err(_) => FixedLengthStream::new_big_int(js_sys::BigInt::from(size))?,
    };
    let pipe = JsFuture::from(body.pipe_to(&fixed.writable()));
    let put = bucket.put(key, Data::ReadableStream(fixed.readable())).http_metadata(meta.http()).custom_metadata(meta.custom()).execute();
    let (piped, put) = join(pipe, put).await;
    piped?;
    put?;
    Ok(())
}

fn content_length(res: &Response, fallback: Option<u64>) -> Option<u64> {
    res.headers().get("content-length").ok().flatten().and_then(|v| v.trim().parse().ok()).or(fallback)
}

async fn put_stream(bucket: &Bucket, key: &str, mut res: Response, fallback: Option<u64>, meta: &Meta) -> DriveResult<()> {
    if let Some(size) = content_length(&res, fallback) {
        if let (_, ResponseBody::Stream(stream)) = res.into_parts() {
            return store_stream(bucket, key, stream, size, meta).await;
        }
        return Ok(());
    }
    let bytes = res.bytes().await?;
    bucket.put(key, Data::Bytes(bytes)).http_metadata(meta.http()).custom_metadata(meta.custom()).execute().await?;
    Ok(())
}

async fn serve_file(req: &Request, url: &Url, file: &DriveFile, drive: &Drive, bucket: &Bucket, ctx: &Context) -> DriveResult<Response> {
    let native = file.mime_type.starts_with("application/vnd.google-apps.");
    let requested = url.query_pairs().find(|(k, _)| k == "export").map(|(_, v)| v.into_owned()).filter(|v| !v.is_empty());
    let target: Option<(String, String)> = if native {
        match requested {
            Some(mime) => Some((mime, String::new())),
            None => export_default(&file.mime_type).map(|(m, e)| (m.to_owned(), e.to_owned())),
        }
    } else {
        None
    };
    if native && target.is_none() {
        return Ok(problem(415, &format!("drive: {} has no export format; pass ?export=<mime>", file.mime_type), &[]));
    }
    let modified = file.modified_time.as_deref();
    let version = match &target {
        Some((mime, _)) => format!("{}/{mime}", modified.unwrap_or("undefined")),
        None => file.md5_checksum.clone().or_else(|| file.modified_time.clone()).unwrap_or_else(|| "0".to_owned()),
    };
    let key = format!("drive/{}/{version}", file.id);
    let name = match &target {
        Some((_, ext)) if !ext.is_empty() => format!("{}.{ext}", file.name),
        _ => file.name.clone(),
    };
    let content_type = match &target {
        Some((mime, _)) => mime.clone(),
        None if file.mime_type.is_empty() => "application/octet-stream".to_owned(),
        None => file.mime_type.clone(),
    };
    let meta = Meta { content_type, disposition: disposition(&name), drive_id: file.id.clone(), name: file.name.clone(), version: version.clone() };
    let is_head = req.method() == Method::Head;

    match cache_get(bucket, &key, req).await? {
        Lookup::Hit { object, precondition_failed } => return from_r2(req, &object, !precondition_failed, "hit"),
        Lookup::Unsatisfiable(size) => return range_not_satisfiable(size),
        Lookup::Miss => {}
    }

    let auth = format!("Bearer {}", drive.token().await?);
    let src = match &target {
        Some((mime, _)) => format!("{}/files/{}/export?mimeType={}", drive.api, encode_uri_component(&file.id), encode_uri_component(mime)),
        None => format!("{}/files/{}?alt=media&supportsAllDrives=true", drive.api, encode_uri_component(&file.id)),
    };
    let file_size: Option<u64> = file.size.as_deref().and_then(|s| s.parse().ok());
    let etag = format!("\"{}\"", file.md5_checksum.as_deref().unwrap_or(&version));
    let miss_headers = || file_headers(Some(&meta.content_type), Some(&meta.disposition), &etag, None, "miss");

    // A Range miss (video seek) goes straight to Drive and fills the cache in the background.
    if let (Some(range), false) = (header(req, "range"), native) {
        let ranged = fetch_raw(&src, Method::Get, &[("authorization", &auth), ("range", &range)], None).await?;
        let fill = {
            let (bucket, key, src, auth) = (bucket.clone(), key.clone(), src.clone(), auth.clone());
            let meta = meta.clone();
            async move {
                match fetch_raw(&src, Method::Get, &[("authorization", &auth)], None).await {
                    Ok(full) if ok(full.status_code()) => {
                        if let Err(e) = put_stream(&bucket, &key, full, file_size, &meta).await {
                            console_error!("drive: cache fill of {key} failed: {e:?}");
                        }
                    }
                    Ok(full) => console_error!("drive: cache fill of {key}: Google returned HTTP {}", full.status_code()),
                    Err(e) => console_error!("drive: cache fill of {key} failed: {e:?}"),
                }
            }
        };
        ctx.wait_until(fill);
        let headers = miss_headers()?;
        for name in ["content-range", "content-length"] {
            if let Some(v) = ranged.headers().get(name).ok().flatten().filter(|v| !v.is_empty()) {
                headers.set(name, &v)?;
            }
        }
        let status = ranged.status_code();
        let (_, body) = ranged.into_parts();
        let body = if is_head { ResponseBody::Empty } else { body };
        return Ok(Response::builder().with_status(status).with_headers(headers).body(body));
    }

    let mut upstream = fetch_raw(&src, Method::Get, &[("authorization", &auth)], None).await?;
    if upstream.status_code() == 404 {
        return Ok(problem(404, "drive: file not found", &[]));
    }
    let status = upstream.status_code();
    if !ok(status) || matches!(upstream.body(), ResponseBody::Empty) {
        let detail: String = upstream.text().await.unwrap_or_default().chars().take(500).collect();
        return Ok(problem(502, &format!("drive: Google returned HTTP {status}"), &[("detail", Value::String(detail))]));
    }

    let size = content_length(&upstream, file_size);
    if let (false, Some(size), ResponseBody::Stream(stream)) = (native, size, upstream.body().clone()) {
        // Stream to the caller and into R2 at once.
        let headers = miss_headers()?;
        headers.set("content-length", &size.to_string())?;
        let body = if is_head {
            let (bucket, key, meta) = (bucket.clone(), key.clone(), meta);
            ctx.wait_until(async move {
                if let Err(e) = store_stream(&bucket, &key, stream, size, &meta).await {
                    console_error!("drive: cache fill of {key} failed: {e:?}");
                }
            });
            ResponseBody::Empty
        } else {
            let pair = stream.tee();
            let (to_client, to_cache): (ReadableStream, ReadableStream) = (pair.get(0).unchecked_into(), pair.get(1).unchecked_into());
            let (bucket, key) = (bucket.clone(), key.clone());
            ctx.wait_until(async move {
                if let Err(e) = store_stream(&bucket, &key, to_cache, size, &meta).await {
                    console_error!("drive: cache fill of {key} failed: {e:?}");
                }
            });
            ResponseBody::Stream(to_client)
        };
        return Ok(Response::builder().with_status(200).with_headers(headers).body(body));
    }

    // Exports (capped at 10 MB by Drive) and bodies of unknown length: store, then serve from R2.
    let bytes = upstream.bytes().await?;
    bucket.put(&key, Data::Bytes(bytes)).http_metadata(meta.http()).custom_metadata(meta.custom()).execute().await?;
    match cache_get(bucket, &key, req).await? {
        Lookup::Hit { object, precondition_failed } => from_r2(req, &object, !precondition_failed, "miss"),
        Lookup::Unsatisfiable(size) => range_not_satisfiable(size),
        Lookup::Miss => Ok(problem(500, "drive: cache write did not persist", &[])),
    }
}

fn json_response(body: String, cache_control: &str) -> DriveResult<Response> {
    let headers = Headers::new();
    headers.set("content-type", "application/json; charset=utf-8")?;
    headers.set("cache-control", cache_control)?;
    Ok(Response::builder().with_status(200).with_headers(headers).body(ResponseBody::Body(body.into_bytes())))
}

fn listing_response(req: &Request, url: &Url, folder: &DriveFile, path: &str, children: &[DriveFile], who: &Identity) -> DriveResult<Response> {
    let entries = entries(&listing_base(path), children);
    let format = url.query_pairs().find(|(k, _)| k == "format").map(|(_, v)| v.into_owned());
    if !wants_html(&header(req, "accept").unwrap_or_default(), format.as_deref()) {
        return json_response(listing_json(folder, path, who, &entries), "private, no-cache");
    }
    let headers = Headers::new();
    headers.set("content-type", "text/html; charset=utf-8")?;
    headers.set("cache-control", "private, no-cache")?;
    Ok(Response::builder().with_status(200).with_headers(headers).body(ResponseBody::Body(listing_html(folder, path, who, &entries).into_bytes())))
}

async fn route(req: &Request, url: &Url, rest: &str, who: &Identity, drive: &Drive, bucket: &Bucket, ctx: &Context) -> DriveResult<Response> {
    if let Some(id) = rest.strip_prefix("_id/") {
        let file = drive.file_meta(&decode_uri_component(id).map_err(|e| DriveErr::Other(e.to_owned()))?).await?;
        if !drive.under_root(&file).await? {
            return Ok(problem(404, "drive: file not found", &[]));
        }
        if file.mime_type == FOLDER {
            return listing_response(req, url, &file, "", &drive.list_children(&file.id).await?, who);
        }
        return serve_file(req, url, &file, drive, bucket, ctx).await;
    }
    let trimmed = rest.trim_end_matches('/');
    let segments: Vec<String> = if trimmed.is_empty() { Vec::new() } else { trimmed.split('/').map(|s| decode_uri_component(s).map_err(|e| DriveErr::Other(e.to_owned()))).collect::<DriveResult<_>>()? };
    let file = drive.resolve_path(&segments).await?;
    if file.mime_type == FOLDER {
        if !segments.is_empty() && !url.path().ends_with('/') {
            let search = url.query().filter(|q| !q.is_empty()).map(|q| format!("?{q}")).unwrap_or_default();
            let target = Url::parse(&format!("{}{}/{search}", url.origin().ascii_serialization(), url.path())).map_err(|e| DriveErr::Other(e.to_string()))?;
            return Ok(Response::redirect_with_status(target, 308)?);
        }
        return listing_response(req, url, &file, &segments.join("/"), &drive.list_children(&file.id).await?, who);
    }
    serve_file(req, url, &file, drive, bucket, ctx).await
}

/// GET/HEAD `/drive/<path>`, `/drive/_id/<fileId>`, `/drive/_whoami`.
pub async fn handle(req: Request, env: Env, ctx: Context) -> Result<Response> {
    if !matches!(req.method(), Method::Get | Method::Head) {
        return Ok(problem(405, "drive: read-only, use GET or HEAD", &[]));
    }
    let who = match access::verify(&req, &env, "drive").await {
        Ok(who) => who,
        Err(denied) => return Ok(denied),
    };
    let url = req.url()?;
    let rest = url.path().strip_prefix(PREFIX).unwrap_or("").trim_start_matches('/').to_owned();
    if rest == "_whoami" {
        let body = serde_json::json!({ "ok": true, "viewer": who }).to_string();
        return json_response(body, "no-store").or_else(|e| Ok(problem(502, &format!("{e:?}"), &[])));
    }
    let (Some(root), Ok(bucket)) = (setting(&env, "DRIVE_ROOT_ID"), env.bucket("DRIVE_CACHE")) else {
        return Ok(problem(503, "drive: DRIVE_CACHE binding or DRIVE_ROOT_ID is not configured", &[]));
    };
    let drive = Drive::from_env(&env, root);
    Ok(match route(&req, &url, &rest, &who, &drive, &bucket, &ctx).await {
        Ok(res) => res,
        Err(DriveErr::NotFound) => problem(404, "drive: file not found", &[("path", Value::String(format!("/{rest}")))]),
        Err(DriveErr::Config(m)) => problem(503, &m, &[]),
        Err(DriveErr::Other(m)) => problem(502, &m, &[]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::{Kind, testutil};
    use rsa::traits::PublicKeyParts;

    fn file(id: &str, name: &str, mime: &str, size: Option<&str>) -> DriveFile {
        DriveFile { id: id.into(), name: name.into(), mime_type: mime.into(), size: size.map(Into::into), md5_checksum: None, modified_time: Some("2026-11-12T00:00:00Z".into()), parents: None }
    }

    fn request_headers(pairs: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        let owned = pairs.to_vec();
        move |n| owned.iter().find(|(k, _)| *k == n).map(|(_, v)| (*v).to_owned())
    }

    fn who() -> Identity {
        Identity { kind: Kind::User, id: "person@example.com".into() }
    }

    #[test]
    fn uri_components_match_javascript() {
        assert_eq!(encode_uri_component("a b/é'(x)*!~"), "a%20b%2F%C3%A9'(x)*!~");
        assert_eq!(encode_uri_component("100%"), "100%25");
        assert_eq!(decode_uri_component("a%20b%2F%C3%A9").unwrap(), "a b/é");
        assert_eq!(decode_uri_component("plain+plus").unwrap(), "plain+plus");
        assert!(decode_uri_component("%").is_err());
        assert!(decode_uri_component("%zz").is_err());
        assert!(decode_uri_component("%C3").is_err());
    }

    #[test]
    fn drive_query_quoting() {
        assert_eq!(quote("it's"), r"'it\'s'");
        assert_eq!(quote(r"a\b"), r"'a\\b'");
        assert_eq!(quote(r"\'"), r"'\\\''");
        let q = drive_query("'root' in parents and trashed=false");
        assert_eq!(
            q,
            "/files?q='root'%20in%20parents%20and%20trashed%3Dfalse&fields=nextPageToken%2Cfiles(id%2Cname%2CmimeType%2Csize%2Cmd5Checksum%2CmodifiedTime%2Cparents)&pageSize=1000&orderBy=folder,name&supportsAllDrives=true&includeItemsFromAllDrives=true"
        );
        assert_eq!(file_path("a/b"), "/files/a%2Fb?fields=id,name,mimeType,size,md5Checksum,modifiedTime,parents&supportsAllDrives=true");
    }

    #[test]
    fn export_defaults() {
        assert_eq!(export_default("application/vnd.google-apps.document"), Some(("text/markdown", "md")));
        assert_eq!(export_default("application/vnd.google-apps.spreadsheet"), Some(("text/csv", "csv")));
        assert_eq!(export_default("application/vnd.google-apps.presentation"), Some(("application/pdf", "pdf")));
        assert_eq!(export_default("application/vnd.google-apps.drawing"), Some(("image/svg+xml", "svg")));
        assert_eq!(export_default("application/vnd.google-apps.script"), Some(("application/vnd.google-apps.script+json", "json")));
        assert_eq!(export_default("application/vnd.google-apps.form"), None);
        assert_eq!(export_default("image/png"), None);
        assert_eq!(disposition("Notes é.md"), "inline; filename*=UTF-8''Notes%20%C3%A9.md");
    }

    #[test]
    fn byte_sizes_round_like_to_fixed() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1280), "1.3 KB"); // 1.25 exactly: toFixed rounds the tie up
        assert_eq!(format_bytes(1_048_575), "1024.0 KB");
        assert_eq!(format_bytes(1_048_576), "1.0 MB");
        assert_eq!(format_bytes(5 * 1024 * 1024 * 1024 + 512 * 1024 * 1024), "5.5 GB");
        assert_eq!(format_bytes(3 * 1024u64.pow(4)), "3.0 TB");
        assert_eq!(format_bytes(2048 * 1024u64.pow(4)), "2048.0 TB");
    }

    #[test]
    fn range_headers_translate_for_r2() {
        assert_eq!(parse_range("bytes=2-4"), Some(Range::OffsetWithLength { offset: 2, length: 3 }));
        assert_eq!(parse_range("bytes=0-0"), Some(Range::OffsetWithLength { offset: 0, length: 1 }));
        assert_eq!(parse_range("bytes=5-"), Some(Range::OffsetToEnd { offset: 5 }));
        assert_eq!(parse_range("bytes=-3"), Some(Range::Suffix { suffix: 3 }));
        assert_eq!(parse_range(" Bytes=1-2 "), Some(Range::OffsetWithLength { offset: 1, length: 2 }));
        assert_eq!(parse_range("bytes=4-2"), None);
        assert_eq!(parse_range("bytes=1-2,5-6"), None);
        assert_eq!(parse_range("bytes=-"), None);
        assert_eq!(parse_range("bytes=a-b"), None);
        assert_eq!(parse_range("items=1-2"), None);
        assert_eq!(parse_range(""), None);
        // above 2^53 the JS number would lose precision: never reach the R2 binding
        assert_eq!(parse_range("bytes=1-99999999999999999999"), None);
        assert_eq!(parse_range("bytes=1-18014398509481984"), Some(Range::OffsetToEnd { offset: 1 }));
        assert_eq!(parse_range("bytes=18014398509481984-"), None);
        assert_eq!(range_window(&Range::OffsetWithLength { offset: 2, length: 3 }, 10), (2, 3));
        assert_eq!(range_window(&Range::OffsetToEnd { offset: 4 }, 10), (4, 6));
        assert_eq!(range_window(&Range::Prefix { length: 99 }, 10), (0, 10));
        assert_eq!(range_window(&Range::Suffix { suffix: 3 }, 10), (7, 3));
        assert_eq!(range_window(&Range::Suffix { suffix: 30 }, 10), (0, 10));
    }

    #[test]
    fn conditional_headers_translate_for_r2() {
        let p = Preconditions::from_headers(request_headers(&[("if-none-match", "\"abc\"")]));
        assert_eq!(p, Preconditions { etag_does_not_match: Some("abc".into()), ..Preconditions::default() });
        let p = Preconditions::from_headers(request_headers(&[("if-none-match", "W/\"abc\"")]));
        assert_eq!(p.etag_does_not_match.as_deref(), Some("abc"));
        let p = Preconditions::from_headers(request_headers(&[("if-match", "\"abc\"")]));
        assert_eq!(p.etag_matches.as_deref(), Some("abc"));
        // seconds-granularity dates become millisecond bounds: 2021-09-02T19:38:31Z = 1630611511 s
        let date = "Thu, 02 Sep 2021 19:38:31 GMT";
        assert_eq!(http_date_ms(date), Some(1_630_611_511_000));
        let p = Preconditions::from_headers(request_headers(&[("if-modified-since", date)]));
        assert_eq!(p.uploaded_after_ms, Some(1_630_611_511_999));
        let p = Preconditions::from_headers(request_headers(&[("if-unmodified-since", date)]));
        assert_eq!(p.uploaded_before_ms, Some(1_630_611_512_000));
        // If-None-Match wins over If-Modified-Since, If-Match over If-Unmodified-Since
        let p = Preconditions::from_headers(request_headers(&[("if-none-match", "\"a\""), ("if-modified-since", date)]));
        assert_eq!((p.etag_does_not_match.as_deref(), p.uploaded_after_ms), (Some("a"), None));
        let p = Preconditions::from_headers(request_headers(&[("if-match", "\"a\""), ("if-unmodified-since", date)]));
        assert_eq!((p.etag_matches.as_deref(), p.uploaded_before_ms), (Some("a"), None));
        // lists and `*` are not expressible, bad dates are ignored
        assert!(Preconditions::from_headers(request_headers(&[("if-none-match", "\"a\", \"b\"")])).is_empty());
        assert!(Preconditions::from_headers(request_headers(&[("if-none-match", "*")])).is_empty());
        assert!(Preconditions::from_headers(request_headers(&[("if-modified-since", "yesterday")])).is_empty());
        assert!(Preconditions::from_headers(request_headers(&[])).is_empty());
    }

    #[test]
    fn etag_lists_and_star_are_checked_locally() {
        assert!(etag_precondition_fails(None, Some("*"), "abc"));
        assert!(etag_precondition_fails(None, Some("\"x\", W/\"abc\""), "abc"));
        assert!(!etag_precondition_fails(None, Some("\"x\", \"y\""), "abc"));
        assert!(!etag_precondition_fails(None, Some("\"abc\""), "abc")); // single tag: R2 decides
        assert!(!etag_precondition_fails(Some("*"), None, "abc"));
        assert!(etag_precondition_fails(Some("\"x\", \"y\""), None, "abc"));
        assert!(!etag_precondition_fails(Some("\"x\", \"abc\""), None, "abc"));
        assert!(!etag_precondition_fails(None, None, "abc"));
    }

    #[test]
    fn r2_responses_follow_from_r2() {
        assert_eq!(plan(false, true, false, None, 10), Plan { status: 200, content_range: None, content_length: Some(10), body: true });
        assert_eq!(plan(true, true, false, None, 10), Plan { status: 200, content_range: None, content_length: Some(10), body: false });
        assert_eq!(plan(false, true, false, Some((2, 3)), 10), Plan { status: 206, content_range: Some("bytes 2-4/10".into()), content_length: Some(3), body: true });
        assert!(!plan(true, true, false, Some((7, 3)), 10).body);
        assert_eq!(plan(false, false, true, None, 10), Plan { status: 304, content_range: None, content_length: None, body: false });
        assert_eq!(plan(false, false, false, None, 10).status, 412);
        assert_eq!(plan(false, true, false, Some((0, 0)), 0).content_range.as_deref(), Some("bytes 0-0/0"));
    }

    #[test]
    fn listing_json_matches_drive_ts() {
        let children = [file("d1", "art work", FOLDER, None), file("f1", "a&b.png", "image/png", Some("1536")), file("g1", "Notes", "application/vnd.google-apps.document", None)];
        let list = entries("/drive/x/", &children);
        let folder = file("root", "Aphrody", FOLDER, None);
        let json = listing_json(&folder, "x", &who(), &list);
        assert_eq!(
            json,
            r#"{"ok":true,"path":"/x","id":"root","name":"Aphrody","viewer":{"kind":"user","id":"person@example.com"},"entries":[{"name":"art work","kind":"dir","mimeType":"application/vnd.google-apps.folder","size":null,"modified":"2026-11-12T00:00:00Z","id":"d1","href":"/drive/x/art%20work/","export":null},{"name":"a&b.png","kind":"file","mimeType":"image/png","size":1536,"modified":"2026-11-12T00:00:00Z","id":"f1","href":"/drive/x/a%26b.png","export":null},{"name":"Notes","kind":"file","mimeType":"application/vnd.google-apps.document","size":null,"modified":"2026-11-12T00:00:00Z","id":"g1","href":"/drive/x/Notes","export":"text/markdown"}]}"#
        );
        assert_eq!(listing_base(""), "/drive/");
        assert_eq!(listing_base("a b/c"), "/drive/a%20b/c/");
    }

    #[test]
    fn listing_html_matches_drive_ts() {
        let folder = file("root", "A <b> folder", FOLDER, None);
        let children = [file("d1", "art", FOLDER, None), file("f1", "a&b.png", "image/png", Some("1536")), file("g1", "Notes", "application/vnd.google-apps.document", None), file("h1", "x", "application/octet-stream", None)];
        let html = listing_html(&folder, "docs/sub dir", &who(), &entries(&listing_base("docs/sub dir"), &children));
        let expected = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>A &lt;b&gt; folder: fw Drive</title><link rel=\"stylesheet\" href=\"/app.css\"><style>\nmain{max-width:56rem;margin:0 auto;padding:24px 16px;font-family:var(--font);color:var(--md-sys-color-on-surface)}\nbody{background:var(--md-sys-color-surface)}nav{color:var(--md-sys-color-on-surface-variant);margin-bottom:16px}\nul{list-style:none;padding:0;margin:0;border-radius:16px;overflow:hidden;background:var(--md-sys-color-surface-container)}\nli{display:flex;justify-content:space-between;gap:16px;min-height:48px;align-items:center;padding:0 16px;border-bottom:1px solid var(--md-sys-color-outline-variant)}\nli:last-child{border-bottom:0}a{color:var(--md-sys-color-primary)}a:focus-visible{outline:3px solid var(--md-sys-color-primary);outline-offset:2px}\nli span{color:var(--md-sys-color-on-surface-variant);font-family:var(--font-code);font-size:14px;white-space:nowrap}p{color:var(--md-sys-color-on-surface-variant)}\n</style></head><body><main><nav><a href=\"/drive/\">Drive</a> / <a href=\"/drive/docs/\">docs</a> / <a href=\"/drive/docs/sub%20dir/\">sub dir</a></nav><h1>A &lt;b&gt; folder</h1><ul><li><a href=\"/drive/docs/sub%20dir/art/\">art/</a><span>Folder</span></li><li><a href=\"/drive/docs/sub%20dir/a%26b.png\">a&amp;b.png</a><span>1.5 KB</span></li><li><a href=\"/drive/docs/sub%20dir/Notes\">Notes</a><span>text/markdown</span></li><li><a href=\"/drive/docs/sub%20dir/x\">x</a><span>application/octet-stream</span></li></ul><p>Signed in as person@example.com. Add <code>?format=json</code> for the machine listing.</p></main></body></html>";
        assert_eq!(html, expected);
        let empty = listing_html(&file("r", "Root", FOLDER, None), "", &who(), &[]);
        assert!(empty.contains("<nav><a href=\"/drive/\">Drive</a></nav><h1>Root</h1><p>This folder is empty."));
        assert!(wants_html("text/html,application/xhtml+xml", None));
        assert!(!wants_html("text/html", Some("json")));
        assert!(!wants_html("application/json", None));
    }

    #[test]
    fn service_account_assertion_signs_and_verifies() {
        let key = testutil::keys(0);
        let unsigned = bearer_assertion("drive-cdn@test.iam.gserviceaccount.com", "https://oauth2.googleapis.com/token", 1_800_000_000);
        let (h, c) = unsigned.split_once('.').unwrap();
        let dec = |s: &str| String::from_utf8(base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, s).unwrap()).unwrap();
        assert_eq!(dec(h), r#"{"alg":"RS256","typ":"JWT"}"#);
        assert_eq!(
            dec(c),
            r#"{"iss":"drive-cdn@test.iam.gserviceaccount.com","scope":"https://www.googleapis.com/auth/drive.readonly","aud":"https://oauth2.googleapis.com/token","iat":1800000000,"exp":1800003600}"#
        );
        // PEM -> PKCS#8 DER -> RS256 signature that the matching public key verifies
        use rsa::pkcs8::EncodePrivateKey;
        let pem = key.to_pkcs8_pem(Default::default()).unwrap();
        let der = pem_to_pkcs8(&pem).unwrap();
        let sig = futures_executor_block(sign_rs256(&der, unsigned.as_bytes())).unwrap();
        assert_eq!(sig.len(), key.to_public_key().size());
        key.to_public_key().pkcs1v15_verify::<rsa::sha2::Sha256>(unsigned.as_bytes(), &sig).unwrap();
        assert!(pem_to_pkcs8("-----BEGIN PRIVATE KEY-----\n!!!\n-----END PRIVATE KEY-----").is_err());
    }

    /// The signing future never yields in native builds; poll it once.
    fn futures_executor_block<T>(fut: impl std::future::Future<Output = T>) -> T {
        let mut fut = std::pin::pin!(fut);
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        match fut.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(v) => v,
            std::task::Poll::Pending => panic!("pure-Rust signing never suspends"),
        }
    }
}
