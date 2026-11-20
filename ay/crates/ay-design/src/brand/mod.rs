// SPDX-License-Identifier: Apache-2.0
//! The brand dataset: the files under `brand/` embedded, parsed and validated.
//!
//! [`character`] holds the sourced character dataset (canon, soul, voice, visual identity, merge log).
//! Names, colours and paths are data in `brand/`; this crate carries none of them.

pub mod character;
pub mod media;

use sha2::{Digest, Sha256};

/// Lowercase hexadecimal SHA-256 of `bytes`, the digest format of the dataset's asset records.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
