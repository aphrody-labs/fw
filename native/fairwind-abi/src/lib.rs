use craft_pe::{DATA_DIRECTORY_NAMES, PeSection, headers, rich_header, subsystem_name};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::slice;

#[cfg(feature = "tree-sitter-all")]
use tree_sitter::{Language, Node, Parser};

pub const ABI_VERSION: u32 = 3;

#[cfg(not(debug_assertions))]
const HEALTH_JSON: &[u8] = b"{\"ok\":true,\"product\":\"fairwind\",\"crate\":\"fairwind-abi\",\"abiVersion\":3,\"profile\":\"release\",\"lto\":\"fat\",\"aot\":true,\"absorbed\":[\"heretic\",\"neon\",\"craft-libs\",\"craft-anything\"],\"capabilities\":[\"tree-sitter\",\"craft-pe\",\"sha256\"],\"treeSitterGrammars\":[\"rust\",\"typescript\",\"tsx\",\"python\",\"c\",\"cpp\",\"java\",\"json\",\"bash\"]}\0";

#[cfg(debug_assertions)]
const HEALTH_JSON: &[u8] = b"{\"ok\":true,\"product\":\"fairwind\",\"crate\":\"fairwind-abi\",\"abiVersion\":3,\"profile\":\"debug\",\"lto\":\"none\",\"aot\":true,\"absorbed\":[\"heretic\",\"neon\",\"craft-libs\",\"craft-anything\"],\"capabilities\":[\"tree-sitter\",\"craft-pe\",\"sha256\"],\"treeSitterGrammars\":[\"rust\",\"typescript\",\"tsx\",\"python\",\"c\",\"cpp\",\"java\",\"json\",\"bash\"]}\0";

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

fn classify_runtime(sections: &[PeSection], has_authenticode: bool, has_clr: bool, file_bytes: u64) -> &'static str {
    let names: Vec<&str> = sections.iter().map(|s| s.name.as_str()).collect();
    if names.iter().any(|n| *n == "google_i" || *n == "protodes" || *n == "flags_he") {
        return "google-antigravity-pe";
    }
    if names.iter().any(|n| *n == ".bun") {
        return "bun-compiled-pe";
    }
    if has_clr {
        return "dotnet-clr-pe";
    }
    if names.iter().any(|n| *n == ".text") && file_bytes < 16 * 1024 * 1024 && !has_authenticode {
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

#[cfg(feature = "tree-sitter-all")]
fn resolve_lang(lang: &str) -> Option<Language> {
    match lang.to_ascii_lowercase().as_str() {
        "rust" | "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
        "typescript" | "ts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "python" | "py" => Some(tree_sitter_python::LANGUAGE.into()),
        "c" => Some(tree_sitter_c::LANGUAGE.into()),
        "cpp" | "c++" | "cc" => Some(tree_sitter_cpp::LANGUAGE.into()),
        "java" => Some(tree_sitter_java::LANGUAGE.into()),
        "json" => Some(tree_sitter_json::LANGUAGE.into()),
        "bash" | "sh" => Some(tree_sitter_bash::LANGUAGE.into()),
        _ => None,
    }
}

#[cfg(feature = "tree-sitter-all")]
fn count_nodes(n: Node<'_>) -> usize {
    let mut c = 1;
    let mut cur = n.walk();
    for ch in n.children(&mut cur) {
        c += count_nodes(ch);
    }
    c
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
    let Some(lang) = resolve_lang(ls) else {
        return 0;
    };
    let src = unsafe { slice::from_raw_parts(sp, sl) };
    let mut p = Parser::new();
    if p.set_language(&lang).is_err() {
        return 0;
    }
    let Some(t) = p.parse(src, None) else {
        return 0;
    };
    let r = t.root_node();
    let j = format!(
        "{{\"ok\":true,\"lang\":\"{}\",\"rootKind\":\"{}\",\"nodeCount\":{},\"hasError\":{}}}",
        ls,
        r.kind(),
        count_nodes(r),
        r.has_error()
    );
    let n = j.len().min(oc);
    unsafe {
        std::ptr::copy_nonoverlapping(j.as_ptr(), ob, n);
    }
    n
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
    if json.is_empty() || json.len() > oc {
        return 0;
    }
    let bytes = json.as_bytes();
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ob, bytes.len());
    }
    bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn abi_ok() {
        assert_eq!(fairwind_abi_version(), 3);
        let s = unsafe { CStr::from_ptr(fairwind_health_json() as *const i8) }
            .to_str()
            .unwrap();
        assert!(s.contains("treeSitterGrammars"));
        assert!(s.contains("craft-pe"));
        assert!(s.contains("\"abiVersion\":3"));
    }

    #[cfg(feature = "tree-sitter-all")]
    #[test]
    fn parse_rust_ok() {
        let (l, s) = (b"rust", b"fn main() { let x = 1 + 2; }");
        let mut buf = [0u8; 256];
        let n = fairwind_parse_summary(
            l.as_ptr(),
            l.len(),
            s.as_ptr(),
            s.len(),
            buf.as_mut_ptr(),
            buf.len(),
        );
        assert!(n > 0);
        let out = std::str::from_utf8(&buf[..n]).unwrap();
        assert!(out.contains("source_file") && out.contains("\"hasError\":false"));
    }

    #[test]
    fn inspect_missing_pe_returns_error_json() {
        let p = b"C:\\nonexistent\\fairwind_missing_binary.exe";
        let mut buf = [0u8; 4096];
        let n = fairwind_inspect_pe(p.as_ptr(), p.len(), buf.as_mut_ptr(), buf.len());
        assert!(n > 0);
        let out = std::str::from_utf8(&buf[..n]).unwrap();
        assert!(out.contains("\"ok\":false"));
    }

    #[test]
    fn inspect_self_test_binary_on_windows() {
        let exe = std::env::current_exe().unwrap();
        if cfg!(windows) {
            let report = inspect_pe_path(&exe).unwrap();
            assert!(report.ok);
            assert!(report.pe32_plus);
            assert_eq!(report.machine, "x86_64");
            assert!(report.section_count > 0);
            assert_eq!(report.sha256.len(), 64);
        }
    }
}
