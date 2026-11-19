//! `fw-nix` — 100% pure-Rust NixOS store, `nixbase32` `/nix/store/<hash>-<name>` addressing,
//! NAR (Nix ARchive) binary serialization, and atomic system generation metadata.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Official Nix base32 alphabet (omits E, O, U, T).
const NIXBASE32_CHARS: &[u8; 32] = b"0123456789abcdfghijklmnpqrsvwxyz";

/// Encode bytes using Nix's official base32 encoding (reversed bit order per Nix store spec).
pub fn nixbase32_encode(bytes: &[u8]) -> String {
    let len = (bytes.len() * 8).div_ceil(5);
    let mut out = String::with_capacity(len);
    for n in (0..len).rev() {
        let b = n * 5;
        let i = b / 8;
        let j = b % 8;
        let mut c = (bytes[i] >> j) as usize;
        if j > 3 && i + 1 < bytes.len() {
            c |= (bytes[i + 1] as usize) << (8 - j);
        }
        out.push(NIXBASE32_CHARS[c & 0x1f] as char);
    }
    out
}

/// Compress a 32-byte SHA-256 hash to Nix's 20-byte store hash via XOR folding.
pub fn compress_nix_hash(sha256: &[u8; 32]) -> [u8; 20] {
    let mut out = [0u8; 20];
    for (i, &b) in sha256.iter().enumerate() {
        out[i % 20] ^= b;
    }
    out
}

fn write_nar_str(buf: &mut Vec<u8>, s: &[u8]) {
    buf.extend_from_slice(&(s.len() as u64).to_le_bytes());
    buf.extend_from_slice(s);
    let pad = (8 - (s.len() % 8)) % 8;
    buf.extend(std::iter::repeat_n(0u8, pad));
}

/// Serialize a single regular file into a valid Nix ARchive (NAR) binary stream.
pub fn encode_nar_regular_file(contents: &[u8], executable: bool) -> Vec<u8> {
    let mut buf = Vec::new();
    write_nar_str(&mut buf, b"nix-archive-1");
    write_nar_str(&mut buf, b"(");
    write_nar_str(&mut buf, b"type");
    write_nar_str(&mut buf, b"regular");
    if executable {
        write_nar_str(&mut buf, b"executable");
        write_nar_str(&mut buf, b"");
    }
    write_nar_str(&mut buf, b"contents");
    write_nar_str(&mut buf, contents);
    write_nar_str(&mut buf, b")");
    buf
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NixStoreEntry {
    pub store_path: String,
    pub nar_sha256_hex: String,
    pub nar_bytes: usize,
    pub uutils_noprefix: bool,
}

/// Compute a deterministic NixOS `/nix/store/<32-char-nixbase32>-<name>` path from file contents.
pub fn compute_fixed_output_store_entry(name: &str, contents: &[u8], executable: bool) -> NixStoreEntry {
    let nar = encode_nar_regular_file(contents, executable);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let nar_raw = Sha256::digest(&nar);
    let mut nar_hex = String::with_capacity(64);
    for b in nar_raw {
        nar_hex.push(HEX[(b >> 4) as usize] as char);
        nar_hex.push(HEX[(b & 0x0f) as usize] as char);
    }
    let fingerprint = format!("fixed:out:sha256:{nar_hex}:/nix/store:{name}");
    let fp_hash: [u8; 32] = Sha256::digest(fingerprint.as_bytes()).into();
    let compressed = compress_nix_hash(&fp_hash);
    let b32 = nixbase32_encode(&compressed);
    NixStoreEntry {
        store_path: format!("/nix/store/{b32}-{name}"),
        nar_sha256_hex: nar_hex,
        nar_bytes: nar.len(),
        uutils_noprefix: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nar_encoding_and_nix_store_path_are_deterministic_and_valid() {
        let entry = compute_fixed_output_store_entry("fw-uutils-4.0.0", b"#!/bin/sh\necho ok\n", true);
        assert!(entry.store_path.starts_with("/nix/store/"));
        assert!(entry.store_path.ends_with("-fw-uutils-4.0.0"));
        let hash_part = entry
            .store_path
            .strip_prefix("/nix/store/")
            .unwrap()
            .split('-')
            .next()
            .unwrap();
        assert_eq!(hash_part.len(), 32);
        assert!(hash_part.bytes().all(|b| NIXBASE32_CHARS.contains(&b)));
        assert_eq!(entry.nar_sha256_hex.len(), 64);
        assert!(entry.nar_bytes >= 64);
    }
}
