#![allow(clippy::not_unsafe_ptr_arg_deref)]

pub mod ast;
pub mod disasm;
pub mod gguf;
pub mod infra;

use craft_pe::{DATA_DIRECTORY_NAMES, PeSection, headers, rich_header, subsystem_name};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::slice;

pub const ABI_VERSION: u32 = 4;

#[cfg(not(debug_assertions))]
const HEALTH_JSON: &[u8] = b"{\"ok\":true,\"product\":\"fairwind\",\"crate\":\"fairwind-abi\",\"abiVersion\":4,\"profile\":\"release\",\"lto\":\"fat\",\"aot\":true,\"absorbed\":[\"heretic\",\"neon\",\"craft-libs\",\"craft-anything\"],\"capabilities\":[\"tree-sitter\",\"tree-sitter-query\",\"tree-sitter-symbols\",\"craft-pe\",\"craft-sniff\",\"iced-x86-disasm\",\"gguf-ggml\",\"rust-crypto\",\"rust-git\",\"rust-ssh\",\"rust-oci\"],\"treeSitterGrammars\":[\"rust\",\"typescript\",\"tsx\",\"javascript\",\"python\",\"c\",\"cpp\",\"java\",\"go\",\"json\",\"bash\"]}\0";

#[cfg(debug_assertions)]
const HEALTH_JSON: &[u8] = b"{\"ok\":true,\"product\":\"fairwind\",\"crate\":\"fairwind-abi\",\"abiVersion\":4,\"profile\":\"debug\",\"lto\":\"none\",\"aot\":true,\"absorbed\":[\"heretic\",\"neon\",\"craft-libs\",\"craft-anything\"],\"capabilities\":[\"tree-sitter\",\"tree-sitter-query\",\"tree-sitter-symbols\",\"craft-pe\",\"craft-sniff\",\"iced-x86-disasm\",\"gguf-ggml\",\"rust-crypto\",\"rust-git\",\"rust-ssh\",\"rust-oci\"],\"treeSitterGrammars\":[\"rust\",\"typescript\",\"tsx\",\"javascript\",\"python\",\"c\",\"cpp\",\"java\",\"go\",\"json\",\"bash\"]}\0";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeDirectoryEntry {
    pub name: &'static str,
    pub rva: u32,
    pub size: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeInspectionReport {
    pub ok: bool,
    pub path: String,
    pub file_bytes: u64,
    pub sha256: String,
    pub pe32_plus: bool,
    pub machine: String,
    pub machine_code: u16,
    pub subsystem: &'static str,
    pub subsystem_code: u16,
    pub entry_rva: u32,
    pub image_base: String,
    pub linker: String,
    pub timestamp: u32,
    pub characteristics: u16,
    pub dll_characteristics: u16,
    pub section_count: usize,
    pub sections: Vec<PeSection>,
    pub data_directories: Vec<PeDirectoryEntry>,
    pub has_authenticode: bool,
    pub authenticode_bytes: u32,
    pub has_debug: bool,
    pub has_tls: bool,
    pub has_clr: bool,
    pub rich_entries: usize,
    pub runtime_kind: &'static str,
}

fn machine_name(machine: u16) -> String {
    match machine {
        0x8664 => "x86_64".to_string(),
        0x014c => "x86".to_string(),
        0xaa64 => "aarch64".to_string(),
        0x01c4 => "armv7".to_string(),
        other => format!("0x{other:04x}"),
    }
}

fn classify_runtime(
    sections: &[PeSection],
    has_authenticode: bool,
    has_clr: bool,
    file_bytes: u64,
) -> &'static str {
    let names: Vec<&str> = sections.iter().map(|s| s.name.as_str()).collect();
    if names.iter().any(|n| *n == "google_i" || *n == "protodes" || *n == "flags_he") {
        return "google-antigravity-pe";
    }
    if names.contains(&".bun") {
        return "bun-compiled-pe";
    }
    if has_clr {
        return "dotnet-clr-pe";
    }
    if names.contains(&".text") && file_bytes < 16 * 1024 * 1024 && !has_authenticode {
        return "native-lto-pe";
    }
    "pe-generic"
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

pub fn inspect_pe_path_opts(path: &Path, compute_hash: bool) -> Result<PeInspectionReport, String> {
    let file = File::open(path).map_err(|e| format!("open failed: {e}"))?;
    let meta = file.metadata().map_err(|e| format!("metadata failed: {e}"))?;
    let file_bytes = meta.len();

    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let mut header_buf = Vec::with_capacity(file_bytes.min(64 * 1024) as usize);
    let sha256 = if compute_hash {
        let mut hasher = Sha256::new();
        let mut chunk = vec![0u8; 256 * 1024];
        loop {
            let n = reader.read(&mut chunk).map_err(|e| format!("read failed: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&chunk[..n]);
            if header_buf.len() < 64 * 1024 {
                let need = (64 * 1024 - header_buf.len()).min(n);
                header_buf.extend_from_slice(&chunk[..need]);
            }
        }
        hex_lower(&hasher.finalize())
    } else {
        let mut take = reader.take(64 * 1024);
        take.read_to_end(&mut header_buf)
            .map_err(|e| format!("read failed: {e}"))?;
        String::new()
    };

    let pe = headers(&header_buf).map_err(|e| format!("pe parse failed: {e}"))?;
    let rich_entries = rich_header(&header_buf, pe.pe_offset).map_or(0, |v| v.len());

    let mut data_directories = Vec::new();
    for (idx, &(rva, size)) in pe.data_dirs.iter().enumerate() {
        if rva != 0 || size != 0 {
            let name = DATA_DIRECTORY_NAMES.get(idx).copied().unwrap_or("unknown");
            data_directories.push(PeDirectoryEntry { name, rva, size });
        }
    }

    let security_dir = pe.dir(4);
    let has_authenticode = security_dir.is_some();
    let authenticode_bytes = security_dir.map_or(0, |(_, sz)| sz);
    let has_debug = pe.dir(6).is_some();
    let has_tls = pe.dir(9).is_some();
    let has_clr = pe.dir(14).is_some();
    let runtime_kind = classify_runtime(&pe.sections, has_authenticode, has_clr, file_bytes);

    Ok(PeInspectionReport {
        ok: true,
        path: path.display().to_string(),
        file_bytes,
        sha256,
        pe32_plus: pe.pe32_plus,
        machine: machine_name(pe.machine),
        machine_code: pe.machine,
        subsystem: subsystem_name(pe.subsystem),
        subsystem_code: pe.subsystem,
        entry_rva: pe.entry_rva,
        image_base: format!("0x{:x}", pe.image_base),
        linker: format!("{}.{}", pe.linker.0, pe.linker.1),
        timestamp: pe.timestamp,
        characteristics: pe.characteristics,
        dll_characteristics: pe.dll_characteristics,
        section_count: pe.sections.len(),
        sections: pe.sections,
        data_directories,
        has_authenticode,
        authenticode_bytes,
        has_debug,
        has_tls,
        has_clr,
        rich_entries,
        runtime_kind,
    })
}

pub fn inspect_pe_path(path: &Path) -> Result<PeInspectionReport, String> {
    inspect_pe_path_opts(path, true)
}

pub fn inspect_pe_headers_fast(path: &Path) -> Result<PeInspectionReport, String> {
    inspect_pe_path_opts(path, false)
}

fn write_json_out(json: &str, ob: *mut u8, oc: usize) -> usize {
    if ob.is_null() || json.is_empty() || json.len() > oc {
        return 0;
    }
    let bytes = json.as_bytes();
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ob, bytes.len());
    }
    bytes.len()
}

#[unsafe(no_mangle)]
pub extern "C" fn fairwind_abi_version() -> u32 {
    ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn fairwind_health_json() -> *const u8 {
    HEALTH_JSON.as_ptr()
}

#[cfg(feature = "tree-sitter-all")]
#[unsafe(no_mangle)]
pub extern "C" fn fairwind_parse_summary(
    lp: *const u8,
    ll: usize,
    sp: *const u8,
    sl: usize,
    ob: *mut u8,
    oc: usize,
) -> usize {
    if lp.is_null() || sp.is_null() || ob.is_null() || oc == 0 {
        return 0;
    }
    let Ok(ls) = std::str::from_utf8(unsafe { slice::from_raw_parts(lp, ll) }) else {
        return 0;
    };
    let src = unsafe { slice::from_raw_parts(sp, sl) };
    let json = match ast::parse_summary(ls, src) {
        Ok(s) => serde_json::to_string(&s).unwrap_or_default(),
        Err(e) => serde_json::json!({ "ok": false, "error": e }).to_string(),
    };
    write_json_out(&json, ob, oc)
}

#[cfg(feature = "tree-sitter-all")]
#[unsafe(no_mangle)]
pub extern "C" fn fairwind_ast_symbols(
    lp: *const u8,
    ll: usize,
    sp: *const u8,
    sl: usize,
    ob: *mut u8,
    oc: usize,
) -> usize {
    if lp.is_null() || sp.is_null() || ob.is_null() || oc == 0 {
        return 0;
    }
    let Ok(ls) = std::str::from_utf8(unsafe { slice::from_raw_parts(lp, ll) }) else {
        return 0;
    };
    let src = unsafe { slice::from_raw_parts(sp, sl) };
    let json = match ast::extract_symbols(ls, src, 256) {
        Ok(s) => serde_json::to_string(&s).unwrap_or_default(),
        Err(e) => serde_json::json!({ "ok": false, "error": e }).to_string(),
    };
    write_json_out(&json, ob, oc)
}

#[cfg(feature = "tree-sitter-all")]
#[unsafe(no_mangle)]
pub extern "C" fn fairwind_ast_query(
    lp: *const u8,
    ll: usize,
    sp: *const u8,
    sl: usize,
    qp: *const u8,
    ql: usize,
    ob: *mut u8,
    oc: usize,
) -> usize {
    if lp.is_null() || sp.is_null() || qp.is_null() || ob.is_null() || oc == 0 {
        return 0;
    }
    let Ok(ls) = std::str::from_utf8(unsafe { slice::from_raw_parts(lp, ll) }) else {
        return 0;
    };
    let Ok(qs) = std::str::from_utf8(unsafe { slice::from_raw_parts(qp, ql) }) else {
        return 0;
    };
    let src = unsafe { slice::from_raw_parts(sp, sl) };
    let json = match ast::run_query(ls, src, qs, 128) {
        Ok(s) => serde_json::to_string(&s).unwrap_or_default(),
        Err(e) => serde_json::json!({ "ok": false, "error": e }).to_string(),
    };
    write_json_out(&json, ob, oc)
}

#[unsafe(no_mangle)]
pub extern "C" fn fairwind_inspect_pe(
    pp: *const u8,
    pl: usize,
    ob: *mut u8,
    oc: usize,
) -> usize {
    if pp.is_null() || ob.is_null() || oc == 0 {
        return 0;
    }
    let Ok(path_str) = std::str::from_utf8(unsafe { slice::from_raw_parts(pp, pl) }) else {
        return 0;
    };
    let json = match inspect_pe_path(Path::new(path_str)) {
        Ok(report) => serde_json::to_string(&report).unwrap_or_default(),
        Err(err) => serde_json::json!({ "ok": false, "path": path_str, "error": err }).to_string(),
    };
    write_json_out(&json, ob, oc)
}

#[unsafe(no_mangle)]
pub extern "C" fn fairwind_identify_file(
    pp: *const u8,
    pl: usize,
    ob: *mut u8,
    oc: usize,
) -> usize {
    if pp.is_null() || ob.is_null() || oc == 0 {
        return 0;
    }
    let Ok(path_str) = std::str::from_utf8(unsafe { slice::from_raw_parts(pp, pl) }) else {
        return 0;
    };
    let json = match infra::identify_file_path(Path::new(path_str)) {
        Ok(report) => serde_json::to_string(&report).unwrap_or_default(),
        Err(err) => serde_json::json!({ "ok": false, "path": path_str, "error": err }).to_string(),
    };
    write_json_out(&json, ob, oc)
}

#[unsafe(no_mangle)]
pub extern "C" fn fairwind_disasm_x64(
    pp: *const u8,
    pl: usize,
    max_insns: usize,
    ob: *mut u8,
    oc: usize,
) -> usize {
    if pp.is_null() || ob.is_null() || oc == 0 {
        return 0;
    }
    let Ok(path_str) = std::str::from_utf8(unsafe { slice::from_raw_parts(pp, pl) }) else {
        return 0;
    };
    let json = match disasm::disassemble_pe_entry(Path::new(path_str), max_insns) {
        Ok(report) => serde_json::to_string(&report).unwrap_or_default(),
        Err(err) => serde_json::json!({ "ok": false, "path": path_str, "error": err }).to_string(),
    };
    write_json_out(&json, ob, oc)
}

#[unsafe(no_mangle)]
pub extern "C" fn fairwind_inspect_gguf(
    pp: *const u8,
    pl: usize,
    ob: *mut u8,
    oc: usize,
) -> usize {
    if pp.is_null() || ob.is_null() || oc == 0 {
        return 0;
    }
    let Ok(path_str) = std::str::from_utf8(unsafe { slice::from_raw_parts(pp, pl) }) else {
        return 0;
    };
    let json = match gguf::inspect_gguf_path(Path::new(path_str)) {
        Ok(report) => serde_json::to_string(&report).unwrap_or_default(),
        Err(err) => serde_json::json!({ "ok": false, "path": path_str, "error": err }).to_string(),
    };
    write_json_out(&json, ob, oc)
}

#[unsafe(no_mangle)]
pub extern "C" fn fairwind_crypto_digest(
    dp: *const u8,
    dl: usize,
    ob: *mut u8,
    oc: usize,
) -> usize {
    if dp.is_null() || ob.is_null() || oc == 0 {
        return 0;
    }
    let data = unsafe { slice::from_raw_parts(dp, dl) };
    let report = infra::compute_crypto_digests(data, None);
    let json = serde_json::to_string(&report).unwrap_or_default();
    write_json_out(&json, ob, oc)
}

#[unsafe(no_mangle)]
pub extern "C" fn fairwind_git_inspect(
    pp: *const u8,
    pl: usize,
    ob: *mut u8,
    oc: usize,
) -> usize {
    if pp.is_null() || ob.is_null() || oc == 0 {
        return 0;
    }
    let Ok(path_str) = std::str::from_utf8(unsafe { slice::from_raw_parts(pp, pl) }) else {
        return 0;
    };
    let json = match infra::inspect_git_repo(Path::new(path_str)) {
        Ok(report) => serde_json::to_string(&report).unwrap_or_default(),
        Err(err) => serde_json::json!({ "ok": false, "error": err }).to_string(),
    };
    write_json_out(&json, ob, oc)
}

#[unsafe(no_mangle)]
pub extern "C" fn fairwind_ssh_fingerprint(
    kp: *const u8,
    kl: usize,
    ob: *mut u8,
    oc: usize,
) -> usize {
    if kp.is_null() || ob.is_null() || oc == 0 {
        return 0;
    }
    let Ok(key_str) = std::str::from_utf8(unsafe { slice::from_raw_parts(kp, kl) }) else {
        return 0;
    };
    let json = match infra::parse_ssh_pubkey_fingerprint(key_str) {
        Ok(report) => serde_json::to_string(&report).unwrap_or_default(),
        Err(err) => serde_json::json!({ "ok": false, "error": err }).to_string(),
    };
    write_json_out(&json, ob, oc)
}

#[unsafe(no_mangle)]
pub extern "C" fn fairwind_ml_dot_f32(ap: *const f32, bp: *const f32, len: usize) -> f32 {
    if ap.is_null() || bp.is_null() || len == 0 {
        return 0.0;
    }
    let a = unsafe { slice::from_raw_parts(ap, len) };
    let b = unsafe { slice::from_raw_parts(bp, len) };
    gguf::dot_f32(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn abi_v4_ok() {
        assert_eq!(fairwind_abi_version(), 4);
        let s = unsafe { CStr::from_ptr(fairwind_health_json() as *const i8) }
            .to_str()
            .unwrap();
        assert!(s.contains("\"abiVersion\":4"));
        assert!(s.contains("craft-sniff"));
        assert!(s.contains("iced-x86-disasm"));
        assert!(s.contains("gguf-ggml"));
        assert!(s.contains("rust-crypto"));
    }

    #[cfg(feature = "tree-sitter-all")]
    #[test]
    fn ast_symbols_and_query_work() {
        let src = b"pub fn hello(x: i32) -> i32 { x + 1 } struct Fairwind { id: u32 }";
        let syms = ast::extract_symbols("rust", src, 32).unwrap();
        assert_eq!(syms.symbol_count, 2);
        assert_eq!(syms.symbols[0].name, "hello");
        assert_eq!(syms.symbols[1].name, "Fairwind");

        let q = ast::run_query("rust", src, "(function_item name: (identifier) @fn)", 16).unwrap();
        assert_eq!(q.match_count, 1);
        assert_eq!(q.matches[0].text, "hello");

        let go_syms = ast::extract_symbols("go", b"package main\nfunc Run() {}", 16).unwrap();
        assert_eq!(go_syms.symbol_count, 1);
        assert_eq!(go_syms.symbols[0].name, "Run");
    }

    #[test]
    fn gguf_parser_and_ggml_kernels_work() {
        let mut synthetic = Vec::new();
        synthetic.extend_from_slice(b"GGUF");
        synthetic.extend_from_slice(&3u32.to_le_bytes());
        synthetic.extend_from_slice(&42u64.to_le_bytes());
        synthetic.extend_from_slice(&1u64.to_le_bytes());
        let key = b"general.architecture";
        synthetic.extend_from_slice(&(key.len() as u64).to_le_bytes());
        synthetic.extend_from_slice(key);
        synthetic.extend_from_slice(&8u32.to_le_bytes());
        let val = b"llama";
        synthetic.extend_from_slice(&(val.len() as u64).to_le_bytes());
        synthetic.extend_from_slice(val);

        let report = gguf::inspect_gguf_bytes(&synthetic, "test.gguf").unwrap();
        assert_eq!(report.version, 3);
        assert_eq!(report.tensor_count, 42);
        assert_eq!(report.metadata.get("general.architecture").map(String::as_str), Some("llama"));

        let d = gguf::dot_f32(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]);
        assert!((d - 32.0).abs() < 1e-5);
        let probs = gguf::softmax_f32(&[1.0, 2.0, 3.0]);
        assert!((probs.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        let cos = gguf::cosine_similarity_f32(&[1.0, 0.0], &[1.0, 0.0]);
        assert!((cos - 1.0).abs() < 1e-5);
    }

    #[test]
    fn rust_infra_crypto_ssh_oci_and_disasm_work() {
        let dig = infra::compute_crypto_digests(b"fairwind", Some(b"secret"));
        assert_eq!(dig.sha256.len(), 64);
        assert_eq!(dig.blake3.len(), 64);
        assert!(dig.hmac_sha256.is_some());

        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"fairwind-ed25519-test-pubkey-bytes");
        let fp = infra::parse_ssh_pubkey_fingerprint(&format!("ssh-ed25519 {b64} user@fairwind")).unwrap();
        assert_eq!(fp.key_type, "ssh-ed25519");
        assert!(fp.sha256_fingerprint.starts_with("SHA256:"));

        let oci = infra::inspect_dockerfile("FROM alpine:3.21\nENV MODE=prod\nEXPOSE 4379\nENTRYPOINT [\"/fairwind\"]\n");
        assert_eq!(oci.base_images, vec!["alpine:3.21"]);
        assert_eq!(oci.exposed_ports, vec!["4379"]);
        assert_eq!(oci.env_keys, vec!["MODE"]);

        if cfg!(windows) {
            let exe = std::env::current_exe().unwrap();
            let dis = disasm::disassemble_pe_entry(&exe, 8).unwrap();
            assert!(dis.ok && dis.count > 0);
            let id = infra::identify_file_path(&exe).unwrap();
            assert!(id.ok && id.bytes > 0);
        }
    }
}
