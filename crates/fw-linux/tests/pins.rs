//! kernel.toml is canonical: what `fw kernel pin` writes is what is committed.

use std::path::PathBuf;

use fw_linux::pins::Pins;

fn kernel_toml() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("kernel.toml")
}

#[test]
fn committed_pins_are_canonical() {
    let path = kernel_toml();
    let pins = Pins::load(&path).expect("kernel.toml loads");
    let committed = std::fs::read_to_string(&path).expect("kernel.toml reads").replace("\r\n", "\n");
    assert_eq!(pins.to_toml().expect("serializes"), committed);
}

#[test]
fn every_tree_has_a_full_commit_and_a_known_profile() {
    let pins = Pins::load(&kernel_toml()).expect("kernel.toml loads");
    assert!(pins.trees.len() >= 4);
    for tree in &pins.trees {
        assert_eq!(tree.commit.len(), 40, "{} commit", tree.id);
        assert!(tree.commit.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(pins.profile.contains_key(&tree.profile));
        assert!(tree.path.starts_with("third_party/"));
    }
}
