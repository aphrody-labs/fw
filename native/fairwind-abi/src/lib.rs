use std::slice;
use tree_sitter::{Language, Node, Parser};

const ABI_VERSION: u32 = 2;
const HEALTH_JSON: &[u8] = b"{\"ok\":true,\"product\":\"fairwind\",\"crate\":\"fairwind-abi\",\"abiVersion\":2,\"absorbed\":[\"heretic\",\"neon\",\"craft-libs\",\"craft-anything\"],\"treeSitterGrammars\":[\"rust\",\"typescript\",\"tsx\",\"python\",\"c\",\"cpp\",\"java\",\"json\",\"bash\"]}\0";

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

fn count_nodes(n: Node<'_>) -> usize {
    let mut c = 1;
    let mut cur = n.walk();
    for ch in n.children(&mut cur) { c += count_nodes(ch); }
    c
}

#[unsafe(no_mangle)]
pub extern "C" fn fairwind_abi_version() -> u32 { ABI_VERSION }

#[unsafe(no_mangle)]
pub extern "C" fn fairwind_health_json() -> *const u8 { HEALTH_JSON.as_ptr() }

#[unsafe(no_mangle)]
pub extern "C" fn fairwind_parse_summary(lp: *const u8, ll: usize, sp: *const u8, sl: usize, ob: *mut u8, oc: usize) -> usize {
    if lp.is_null() || sp.is_null() || ob.is_null() || oc == 0 { return 0; }
    let Ok(ls) = std::str::from_utf8(unsafe { slice::from_raw_parts(lp, ll) }) else { return 0; };
    let Some(lang) = resolve_lang(ls) else { return 0; };
    let src = unsafe { slice::from_raw_parts(sp, sl) };
    let mut p = Parser::new();
    if p.set_language(&lang).is_err() { return 0; }
    let Some(t) = p.parse(src, None) else { return 0; };
    let r = t.root_node();
    let j = format!("{{\"ok\":true,\"lang\":\"{}\",\"rootKind\":\"{}\",\"nodeCount\":{},\"hasError\":{}}}", ls, r.kind(), count_nodes(r), r.has_error());
    let n = j.len().min(oc);
    unsafe { std::ptr::copy_nonoverlapping(j.as_ptr(), ob, n); }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;
    #[test]
    fn abi_ok() {
        assert_eq!(fairwind_abi_version(), 2);
        let s = unsafe { CStr::from_ptr(fairwind_health_json() as *const i8) }.to_str().unwrap();
        assert!(s.contains("treeSitterGrammars"));
    }
    #[test]
    fn parse_rust_ok() {
        let (l, s) = (b"rust", b"fn main() { let x = 1 + 2; }");
        let mut buf = [0u8; 256];
        let n = fairwind_parse_summary(l.as_ptr(), l.len(), s.as_ptr(), s.len(), buf.as_mut_ptr(), buf.len());
        assert!(n > 0);
        let out = std::str::from_utf8(&buf[..n]).unwrap();
        assert!(out.contains("source_file") && out.contains("\"hasError\":false"));
    }
}
