use know_rust::*;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn scan_items_emits_facts_on_fixture() {
    let fixture = fixture_root();
    let out_dir = TempDir::new().expect("create tempdir");

    scan_items(fixture.as_path(), out_dir.path()).expect("scan_items succeeds");

    let items_path = out_dir.path().join("know_rust_items.json");
    let json = std::fs::read_to_string(&items_path).expect("read know_rust_items.json");
    let facts: ItemFacts = serde_json::from_str(&json).expect("parse ItemFacts");

    assert!(facts.files_scanned > 0, "files_scanned > 0");
    assert!(!facts.impls.is_empty(), "at least one impl recorded");
    assert!(!facts.traits.is_empty(), "at least one trait recorded");
    assert!(!facts.types.is_empty(), "at least one struct/enum recorded");
    assert!(!facts.fns.is_empty(), "at least one fn recorded");
    assert!(!facts.derives.is_empty(), "at least one derive recorded");

    let renderer_trait = facts.traits.iter().find(|t| t.name == "Renderer");
    assert!(
        renderer_trait.is_some(),
        "fixture's Renderer trait surfaces"
    );

    let widget_struct = facts.types.iter().find(|t| t.name == "Widget");
    assert!(
        widget_struct.is_some(),
        "fixture's Widget struct surfaces"
    );

    let debug_derive = facts
        .derives
        .iter()
        .find(|d| d.trait_name == "Debug");
    assert!(
        debug_derive.is_some(),
        "fixture's #[derive(Debug, Clone)] on Frame surfaces Debug"
    );
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("mini_workspace")
}
