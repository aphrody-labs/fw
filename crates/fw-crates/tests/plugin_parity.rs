//! The Bun plugin (`src/crates.ts`) lists the same MCP tools as the engine; this keeps the two from drifting.

#[test]
fn bun_plugin_declares_every_engine_tool_verbatim() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/crates.ts");
    let ts = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    for (name, description, schema) in fw_crates::mcp_tools() {
        assert!(ts.contains(&format!("name: \"{name}\"")), "{name} missing from src/crates.ts");
        assert!(ts.contains(description), "description of {name} differs between fw-crates and src/crates.ts");
        for prop in schema["properties"].as_object().into_iter().flatten().map(|(k, _)| k) {
            assert!(ts.contains(&format!("{prop}:")), "{name}: property {prop} missing from src/crates.ts");
        }
    }
}
