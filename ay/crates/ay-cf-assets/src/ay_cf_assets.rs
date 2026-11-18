#![doc = "Production pure-Rust reusable Cloudflare R2 asset validation, SPDX compliance, HTTP Range, glTF/GLB, KTX2, rect atlas, and ThumbHash helpers."]
#![forbid(unsafe_code)]

use http::HeaderMap;
use http::header::{ACCEPT_RANGES, CACHE_CONTROL, CONTENT_TYPE};
use http_range_header::parse_range_header;
use serde::{Deserialize, Serialize};
use spdx::Expression;
use thiserror::Error;
use url::Url;

/// Cloudflare Free/Pro/Business edge cache object size ceiling (512 MiB).
pub const CLOUDFLARE_EDGE_CACHE_MAX_BYTES: u64 = 512 * 1024 * 1024;

/// Immutable Cache-Control directive for content-addressed R2 blobs.
pub const IMMUTABLE_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

/// Errors returned by reusable R2 asset catalog validation.
#[derive(Debug, Error)]
pub enum AssetError {
    /// SPDX license expression failed to parse.
    #[error("invalid SPDX license expression `{license}`: {reason}")]
    InvalidSpdx {
        /// Original license string.
        license: String,
        /// Parse failure detail.
        reason: String,
    },
    /// SPDX license violates commercial/open redistribution policy.
    #[error("rejected non-permissive or restricted license `{0}`")]
    RejectedLicense(String),
    /// Object size exceeds Cloudflare CDN single-object cacheability ceiling.
    #[error("asset `{id}` size {bytes} exceeds 512 MiB edge cache ceiling ({max_bytes})")]
    ExceedsEdgeCacheLimit {
        /// Catalog asset identifier.
        id: String,
        /// Payload size in bytes.
        bytes: u64,
        /// Allowed ceiling.
        max_bytes: u64,
    },
    /// CC-BY tier license requires non-empty attribution metadata.
    #[error("asset `{id}` under `{spdx}` must include attribution text")]
    MissingAttribution {
        /// Catalog asset identifier.
        id: String,
        /// License identifier.
        spdx: String,
    },
    /// Asset binary container failed format header validation.
    #[error("invalid {kind} container: {detail}")]
    InvalidContainer {
        /// Asset format kind.
        kind: &'static str,
        /// Underlying parser error.
        detail: String,
    },
    /// Origin URL is malformed or non-HTTPS.
    #[error("invalid HTTPS origin URL `{0}`")]
    InvalidOriginUrl(String),
}

/// Supported reusable media & 3D asset categories in `aphrody-assets` and `aphrody-video`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    /// Sanitized vector icons (`svg`).
    Icon,
    /// Spritesheet / UI packs (`png`, `svg`, `webp`).
    Ui,
    /// Equirectangular HDRI environments & backplates (`exr`, `hdr`, `webp`).
    Hdri,
    /// Loopable AV1/WebM video streams (`mp4`, `webm`).
    Loop,
    /// Vector Lottie animations (`json`).
    Lottie,
    /// GPU block-compressed / BasisU mipmapped textures (`ktx2`).
    Tex,
    /// Self-contained binary glTF 2.0 3D models (`glb`).
    Glb,
}

impl AssetKind {
    /// Returns the path segment used inside `v1/<kind>/<blake3>.<ext>`.
    #[must_use]
    pub const fn path_segment(self) -> &'static str {
        match self {
            Self::Icon => "icon",
            Self::Ui => "ui",
            Self::Hdri => "hdri",
            Self::Loop => "loop",
            Self::Lottie => "lottie",
            Self::Tex => "tex",
            Self::Glb => "glb",
        }
    }
}

/// Verified metadata record for a content-addressed R2 object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct R2AssetRecord {
    /// Logical upstream asset identifier (e.g. `polyhaven/hdri/puresky/8k`).
    pub id: String,
    /// Asset domain category.
    pub kind: AssetKind,
    /// Hex-encoded BLAKE3 digest of stored payload bytes.
    pub blake3_hex: String,
    /// Exact payload length in bytes.
    pub bytes: u64,
    /// File extension without leading dot.
    pub ext: String,
    /// Canonical MIME type.
    pub mime: String,
    /// Validated SPDX license expression.
    pub spdx: String,
    /// Attribution notice when required by `CC-BY-*`.
    pub attribution: Option<String>,
    /// Verified upstream HTTPS source URL.
    pub source_url: String,
}

impl R2AssetRecord {
    /// Derives the deterministic BLAKE3 object key `v1/<kind>/<blake3>.<ext>`.
    #[must_use]
    pub fn object_key(&self) -> String {
        format!(
            "v1/{}/{}.{}",
            self.kind.path_segment(),
            self.blake3_hex,
            self.ext.trim_start_matches('.')
        )
    }

    /// Validates SPDX compliance, attribution requirements, URL, and Cloudflare 512 MiB cap.
    ///
    /// # Errors
    /// Returns [`AssetError`] if license, attribution, URL, or edge cache ceiling checks fail.
    pub fn validate(&self) -> Result<(), AssetError> {
        let parsed = Url::parse(&self.source_url)
            .map_err(|_| AssetError::InvalidOriginUrl(self.source_url.clone()))?;
        if parsed.scheme() != "https" {
            return Err(AssetError::InvalidOriginUrl(self.source_url.clone()));
        }
        verify_permissive_spdx(&self.spdx)?;
        if self.spdx.contains("CC-BY-")
            && self
                .attribution
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
        {
            return Err(AssetError::MissingAttribution {
                id: self.id.clone(),
                spdx: self.spdx.clone(),
            });
        }
        if self.bytes > CLOUDFLARE_EDGE_CACHE_MAX_BYTES {
            return Err(AssetError::ExceedsEdgeCacheLimit {
                id: self.id.clone(),
                bytes: self.bytes,
                max_bytes: CLOUDFLARE_EDGE_CACHE_MAX_BYTES,
            });
        }
        Ok(())
    }

    /// Builds recommended Cloudflare R2 HTTP response headers using `http::HeaderMap`.
    #[must_use]
    pub fn response_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(cache) = IMMUTABLE_CACHE_CONTROL.parse() {
            headers.insert(CACHE_CONTROL, cache);
        }
        if let Ok(ct) = self.mime.parse() {
            headers.insert(CONTENT_TYPE, ct);
        }
        if let Ok(ranges) = "bytes".parse() {
            headers.insert(ACCEPT_RANGES, ranges);
        }
        headers
    }
}

/// Computes lowercase hex BLAKE3 digest over an in-memory payload.
#[must_use]
pub fn blake3_hex(payload: &[u8]) -> String {
    blake3::hash(payload).to_hex().to_string()
}

/// Parses and validates an SPDX license expression against R2 commercial redistribution rules.
///
/// Only `CC0-1.0`, `MIT`, `ISC`, `Apache-2.0`, `Zlib`, `BSD-*`, `OFL-1.1`, and `CC-BY-3.0`/`4.0`
/// are accepted; `-NC`, `-SA`, `GPL`, `AGPL`, `LGPL`, and proprietary/EULA terms are rejected.
///
/// # Errors
/// Returns [`AssetError`] if parsing fails or any disallowed requirement appears.
pub fn verify_permissive_spdx(expr_str: &str) -> Result<(), AssetError> {
    let expr = Expression::parse(expr_str).map_err(|err| AssetError::InvalidSpdx {
        license: expr_str.to_owned(),
        reason: err.to_string(),
    })?;

    for req in expr.requirements() {
        let id = req.req.to_string();
        let allowed = matches!(
            id.as_str(),
            "CC0-1.0"
                | "MIT"
                | "ISC"
                | "Apache-2.0"
                | "Apache-2.0 WITH LLVM-exception"
                | "Zlib"
                | "BSD-2-Clause"
                | "BSD-3-Clause"
                | "OFL-1.1"
                | "CC-BY-4.0"
                | "CC-BY-3.0"
                | "Unlicense"
        );
        if !allowed {
            return Err(AssetError::RejectedLicense(id));
        }
    }
    Ok(())
}

/// Validates a KTX2 GPU texture header via the `ktx2` crate and returns `(width, height, mip_levels)`.
///
/// # Errors
/// Returns [`AssetError::InvalidContainer`] if the payload is not a valid KTX2 container.
pub fn inspect_ktx2(bytes: &[u8]) -> Result<(u32, u32, u32), AssetError> {
    let reader = ktx2::Reader::new(bytes).map_err(|err| AssetError::InvalidContainer {
        kind: "ktx2",
        detail: format!("{err:?}"),
    })?;
    let hdr = reader.header();
    Ok((hdr.pixel_width, hdr.pixel_height, hdr.level_count))
}

/// Validates a `.glb` binary glTF 2.0 container via `gltf::Glb::from_slice` and returns JSON + binary byte sizes.
///
/// # Errors
/// Returns [`AssetError::InvalidContainer`] if the slice is not a valid GLB 2.0 stream.
pub fn inspect_glb(bytes: &[u8]) -> Result<(usize, usize), AssetError> {
    let glb = gltf::Glb::from_slice(bytes).map_err(|err| AssetError::InvalidContainer {
        kind: "glb",
        detail: err.to_string(),
    })?;
    let bin_len = glb.bin.as_ref().map_or(0, |b| b.len());
    Ok((glb.json.len(), bin_len))
}

/// Resolves an HTTP `Range` header against `total_len` using RFC-compliant `http-range-header`.
///
/// Returns `Ok(Some((start, end_inclusive)))` for a single byte range, `Ok(None)` when absent or invalid.
#[must_use]
pub fn resolve_single_byte_range(header_value: &str, total_len: u64) -> Option<(u64, u64)> {
    if total_len == 0 {
        return None;
    }
    let parsed = parse_range_header(header_value).ok()?;
    let validated = parsed.validate(total_len).ok()?;
    validated.first().map(|r| (*r.start(), *r.end()))
}

/// Packs sprite/icon rectangles into an atlas using `etagere::AtlasAllocator`.
#[must_use]
pub fn pack_icon_atlas(width: i32, height: i32, rects: &[(i32, i32)]) -> Vec<(i32, i32, i32, i32)> {
    let mut atlas = etagere::AtlasAllocator::new(etagere::size2(width, height));
    rects
        .iter()
        .filter_map(|&(w, h)| {
            let alloc = atlas.allocate(etagere::size2(w, h))?;
            Some((
                alloc.rectangle.min.x,
                alloc.rectangle.min.y,
                alloc.rectangle.width(),
                alloc.rectangle.height(),
            ))
        })
        .collect()
}

/// Encodes a tiny RGBA preview image into a compact ThumbHash byte vector (`thumbhash` crate).
#[must_use]
pub fn encode_thumbhash(width: usize, height: usize, rgba: &[u8]) -> Vec<u8> {
    thumbhash::rgba_to_thumb_hash(width, height, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_spdx_allowlist_and_attribution() {
        assert!(verify_permissive_spdx("CC0-1.0").is_ok());
        assert!(verify_permissive_spdx("MIT OR Apache-2.0").is_ok());
        assert!(verify_permissive_spdx("CC-BY-NC-4.0").is_err());
        assert!(verify_permissive_spdx("GPL-3.0-only").is_err());

        let record = R2AssetRecord {
            id: "noto/emoji/1f600".into(),
            kind: AssetKind::Lottie,
            blake3_hex: blake3_hex(b"{}"),
            bytes: 2,
            ext: "json".into(),
            mime: "application/json".into(),
            spdx: "CC-BY-4.0".into(),
            attribution: None,
            source_url: "https://fonts.gstatic.com/s/e/notoemoji/latest/1f600/lottie.json".into(),
        };
        assert!(matches!(
            record.validate(),
            Err(AssetError::MissingAttribution { .. })
        ));

        let valid = R2AssetRecord {
            attribution: Some("Google Fonts Noto Emoji Animation".into()),
            ..record
        };
        assert!(valid.validate().is_ok());
        assert!(valid.object_key().starts_with("v1/lottie/"));
    }

    #[test]
    fn parses_http_range_and_packs_atlas_and_thumbhash() {
        assert_eq!(
            resolve_single_byte_range("bytes=0-1023", 4096),
            Some((0, 1023))
        );
        assert_eq!(
            resolve_single_byte_range("bytes=-512", 4096),
            Some((3584, 4095))
        );

        let slots = pack_icon_atlas(128, 128, &[(32, 32), (32, 32), (64, 64)]);
        assert_eq!(slots.len(), 3);

        let rgba = vec![255u8; 4 * 4 * 4];
        let hash = encode_thumbhash(4, 4, &rgba);
        assert!(!hash.is_empty());
    }

    #[test]
    fn validates_minimal_glb_header() -> Result<(), Box<dyn std::error::Error>> {
        let json_bytes = br#"{"asset":{"version":"2.0"}} "#;
        let json_len = u32::try_from(json_bytes.len()).unwrap_or(0);
        let total_len = 12 + 8 + json_len;
        let mut glb = Vec::new();
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&total_len.to_le_bytes());
        glb.extend_from_slice(&json_len.to_le_bytes());
        glb.extend_from_slice(&0x4e4f_534au32.to_le_bytes());
        glb.extend_from_slice(json_bytes);

        let (jlen, blen) = inspect_glb(&glb)?;
        assert_eq!(jlen, json_bytes.len());
        assert_eq!(blen, 0);
        assert!(inspect_ktx2(b"not-ktx2").is_err());
        Ok(())
    }
}
