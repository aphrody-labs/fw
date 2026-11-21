//! Fetching, verifying and unpacking the pinned images; recording their licenses.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::pins::{Image, Pins};

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

fn agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(None::<Duration>)
        .timeout_connect(Some(Duration::from_secs(30)))
        .build();
    ureq::Agent::new_with_config(config)
}

/// Downloads `image` into `downloads` unless a file with the pinned checksum is already there.
pub fn download(image: &Image, downloads: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(downloads).map_err(|error| format!("{}: {error}", downloads.display()))?;
    let target = downloads.join(&image.filename);
    if target.is_file() && sha256_file(&target).map_err(|error| error.to_string())? == image.sha256 {
        return Ok(target);
    }
    let partial = downloads.join(format!("{}.part", image.filename));
    let response = agent().get(&image.url).call().map_err(|error| format!("{}: {error}", image.url))?;
    let mut reader = response.into_body().into_reader();
    let mut file = File::create(&partial).map_err(|error| format!("{}: {error}", partial.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = reader.read(&mut buffer).map_err(|error| format!("{}: {error}", image.url))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read]).map_err(|error| error.to_string())?;
    }
    drop(file);
    let digest = hex(&hasher.finalize());
    if digest != image.sha256 {
        let _removed = fs::remove_file(&partial);
        return Err(format!("{}: sha256 {digest} does not match the pin {}", image.filename, image.sha256));
    }
    fs::rename(&partial, &target).map_err(|error| error.to_string())?;
    Ok(target)
}

/// Extracts the pinned member of `archive` to `images/<role>.img`, skipping it when the size already matches.
pub fn extract(image: &Image, archive: &Path, images: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(images).map_err(|error| error.to_string())?;
    let target = images.join(format!("{}.img", image.role));
    let file = File::open(archive).map_err(|error| format!("{}: {error}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|error| format!("{}: {error}", archive.display()))?;
    let mut member = zip.by_name(&image.member).map_err(|error| format!("{}: {error}", image.member))?;
    if fs::metadata(&target).map(|meta| meta.len() == member.size()).unwrap_or(false) {
        return Ok(target);
    }
    let partial = images.join(format!("{}.img.part", image.role));
    let mut out = File::create(&partial).map_err(|error| error.to_string())?;
    io::copy(&mut member, &mut out).map_err(|error| error.to_string())?;
    drop(out);
    fs::rename(&partial, &target).map_err(|error| error.to_string())?;
    Ok(target)
}

/// Fetches and unpacks every pinned image under `state` and writes `images/LICENSES.json`. Returns the image paths.
pub fn ensure(pins: &Pins, state: &Path) -> Result<Vec<PathBuf>, String> {
    let images_dir = state.join("images");
    let mut paths = Vec::new();
    for image in &pins.images {
        let archive = download(image, &state.join("downloads"))?;
        paths.push(extract(image, &archive, &images_dir)?);
    }
    let record = serde_json::json!({
        "images": pins.images.iter().map(|image| serde_json::json!({
            "role": image.role, "filename": image.filename, "sha256": image.sha256, "url": image.url,
        })).collect::<Vec<_>>(),
        "licenses": pins.licenses,
    });
    let text = serde_json::to_string_pretty(&record).map_err(|error| error.to_string())?;
    fs::write(images_dir.join("LICENSES.json"), text).map_err(|error| error.to_string())?;
    Ok(paths)
}

#[derive(Debug, Deserialize)]
pub struct OtaEntry {
    pub filename: String,
    pub id: String,
    pub size: u64,
    pub datetime: u64,
}

#[derive(Deserialize)]
struct OtaResponse {
    response: Vec<OtaEntry>,
}

/// The newest entry of an OTA channel json (what `fw android status` compares the pin with).
pub fn newest(channel_url: &str) -> Result<OtaEntry, String> {
    let body = ureq::get(channel_url)
        .call()
        .map_err(|error| format!("{channel_url}: {error}"))?
        .into_body()
        .read_to_string()
        .map_err(|error| format!("{channel_url}: {error}"))?;
    let parsed: OtaResponse = serde_json::from_str(&body).map_err(|error| format!("{channel_url}: {error}"))?;
    parsed.response.into_iter().max_by_key(|entry| entry.datetime).ok_or_else(|| format!("{channel_url}: empty channel"))
}
