use base64::Engine;
use craft_sniff::{HEAD, identify};
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::{Digest, Sha256, Sha512};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileIdentifyReport {
    pub ok: bool,
    pub path: String,
    pub bytes: u64,
    pub format: String,
    pub mime: String,
    pub extension: String,
    pub family: String,
    pub is_gguf: bool,
    pub shannon_entropy: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CryptoDigestReport {
    pub ok: bool,
    pub sha256: String,
    pub sha512: String,
    pub blake3: String,
    pub hmac_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshFingerprintReport {
    pub ok: bool,
    pub key_type: String,
    pub comment: String,
    pub sha256_fingerprint: String,
    pub key_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitInspectReport {
    pub ok: bool,
    pub repo_root: String,
    pub branch: Option<String>,
    pub head_ref: String,
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OciDockerfileReport {
    pub ok: bool,
    pub base_images: Vec<String>,
    pub exposed_ports: Vec<String>,
    pub env_keys: Vec<String>,
    pub entrypoint: Option<String>,
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn shannon_entropy(sample: &[u8]) -> f64 {
    if sample.is_empty() {
        return 0.0;
    }
    let mut counts = [0u64; 256];
    for &b in sample {
        counts[b as usize] += 1;
    }
    let len = sample.len() as f64;
    let mut h = 0.0;
    for &c in &counts {
        if c > 0 {
            let p = (c as f64) / len;
            h -= p * p.log2();
        }
    }
    (h * 1000.0).round() / 1000.0
}

pub fn identify_file_path(path: &Path) -> Result<FileIdentifyReport, String> {
    let mut f = File::open(path).map_err(|e| format!("open failed: {e}"))?;
    let bytes = f.metadata().map_err(|e| format!("metadata failed: {e}"))?.len();
    let sample_cap = HEAD.max(16 * 1024);
    let mut sample = vec![0u8; sample_cap];
    let n = f.read(&mut sample).map_err(|e| format!("read failed: {e}"))?;
    sample.truncate(n);

    let is_gguf = sample.starts_with(b"GGUF");
    let entropy = shannon_entropy(&sample);

    if is_gguf {
        return Ok(FileIdentifyReport {
            ok: true,
            path: path.display().to_string(),
            bytes,
            format: "GGUF".to_string(),
            mime: "application/x-gguf".to_string(),
            extension: "gguf".to_string(),
            family: "ml-model".to_string(),
            is_gguf: true,
            shannon_entropy: entropy,
        });
    }

    if let Some(kind) = identify(&sample) {
        return Ok(FileIdentifyReport {
            ok: true,
            path: path.display().to_string(),
            bytes,
            format: format!("{kind:?}"),
            mime: kind.mime_type().to_string(),
            extension: kind.extension().to_string(),
            family: format!("{:?}", kind.family()),
            is_gguf: false,
            shannon_entropy: entropy,
        });
    }

    let is_text = !sample.iter().take(512).any(|&b| b == 0);
    Ok(FileIdentifyReport {
        ok: true,
        path: path.display().to_string(),
        bytes,
        format: if is_text { "Text".to_string() } else { "Binary".to_string() },
        mime: if is_text {
            "text/plain".to_string()
        } else {
            "application/octet-stream".to_string()
        },
        extension: path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string(),
        family: if is_text { "text".to_string() } else { "binary".to_string() },
        is_gguf: false,
        shannon_entropy: entropy,
    })
}

pub fn compute_crypto_digests(data: &[u8], hmac_key: Option<&[u8]>) -> CryptoDigestReport {
    let sha256 = hex_lower(&Sha256::digest(data));
    let sha512 = hex_lower(&Sha512::digest(data));
    let blake3 = blake3::hash(data).to_hex().to_string();
    let hmac_sha256 = hmac_key.and_then(|k| {
        let mut mac = HmacSha256::new_from_slice(k).ok()?;
        mac.update(data);
        Some(hex_lower(&mac.finalize().into_bytes()))
    });
    CryptoDigestReport {
        ok: true,
        sha256,
        sha512,
        blake3,
        hmac_sha256,
    }
}

pub fn parse_ssh_pubkey_fingerprint(pubkey_line: &str) -> Result<SshFingerprintReport, String> {
    let mut parts = pubkey_line.split_whitespace();
    let key_type = parts.next().ok_or("missing ssh key type")?.to_string();
    let b64 = parts.next().ok_or("missing ssh base64 key blob")?;
    let comment = parts.collect::<Vec<_>>().join(" ");
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("invalid base64 key blob: {e}"))?;
    let digest = Sha256::digest(&decoded);
    let fp = base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest);
    Ok(SshFingerprintReport {
        ok: true,
        key_type,
        comment,
        sha256_fingerprint: format!("SHA256:{fp}"),
        key_bytes: decoded.len(),
    })
}

pub fn inspect_git_repo(repo_root: &Path) -> Result<GitInspectReport, String> {
    let git_dir = repo_root.join(".git");
    let head_path = git_dir.join("HEAD");
    let head_raw = fs::read_to_string(&head_path)
        .map_err(|e| format!("read .git/HEAD failed: {e}"))?
        .trim()
        .to_string();
    if let Some(ref_name) = head_raw.strip_prefix("ref: ") {
        let branch = ref_name.strip_prefix("refs/heads/").map(str::to_string);
        let ref_file = git_dir.join(ref_name);
        let commit = fs::read_to_string(ref_file)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Ok(GitInspectReport {
            ok: true,
            repo_root: repo_root.display().to_string(),
            branch,
            head_ref: ref_name.to_string(),
            commit,
        })
    } else {
        Ok(GitInspectReport {
            ok: true,
            repo_root: repo_root.display().to_string(),
            branch: None,
            head_ref: "DETACHED".to_string(),
            commit: Some(head_raw),
        })
    }
}

pub fn inspect_dockerfile(content: &str) -> OciDockerfileReport {
    let mut base_images = Vec::new();
    let mut exposed_ports = Vec::new();
    let mut env_keys = Vec::new();
    let mut entrypoint = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, char::is_whitespace);
        let instr = parts.next().unwrap_or("").to_ascii_uppercase();
        let rest = parts.next().unwrap_or("").trim();
        match instr.as_str() {
            "FROM" => {
                if let Some(img) = rest.split_whitespace().find(|t| !t.starts_with("--")) {
                    base_images.push(img.to_string());
                }
            }
            "EXPOSE" => {
                for p in rest.split_whitespace() {
                    exposed_ports.push(p.to_string());
                }
            }
            "ENV" => {
                if let Some(first) = rest.split_whitespace().next() {
                    let key = first.split('=').next().unwrap_or(first);
                    env_keys.push(key.to_string());
                }
            }
            "ENTRYPOINT" | "CMD" => {
                if entrypoint.is_none() {
                    entrypoint = Some(rest.to_string());
                }
            }
            _ => {}
        }
    }

    OciDockerfileReport {
        ok: true,
        base_images,
        exposed_ports,
        env_keys,
        entrypoint,
    }
}
