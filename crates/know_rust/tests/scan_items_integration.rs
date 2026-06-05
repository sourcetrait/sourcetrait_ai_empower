//! Integration tests for `know_rust scan items`. Covers:
//! - workspace-level scan_items orchestration on a fixture workspace
//! - per-source-string walker behavior (port of test_orientation.py's
//!   six scan-level tests, lines 24-72)

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

// --- Per-source-string walker tests (port of test_orientation.py's
// six scan-level tests, lines 24-72) ---

#[test]
fn masking_excludes_decoys() {
    let src = r#"
// impl Command for InComment {}
/* impl Command for InBlock {} */
fn f() { let s = "impl Command for InString {}"; let c = '}'; let l: &'static str = ""; }
impl Command for Real {}
"#;
    let f = scan_source("t.rs", src).expect("parse ok");
    let impls: Vec<(Option<String>, Option<String>)> = f
        .impls
        .iter()
        .map(|i| (i.trait_name.clone(), i.type_name.clone()))
        .collect();
    assert_eq!(
        impls,
        vec![(Some("Command".to_string()), Some("Real".to_string()))]
    );
}

#[test]
fn macro_args_captured() {
    let f = scan_source("t.rs", "fn s() { bind_command!(ws, A, B, C); }").expect("parse ok");
    let m = f
        .macros
        .iter()
        .find(|x| x.name == "bind_command")
        .expect("bind_command macro present");
    let arg_idents = m.arg_idents.as_ref().expect("arg_idents recorded");
    assert_eq!(
        arg_idents,
        &vec![
            "ws".to_string(),
            "A".to_string(),
            "B".to_string(),
            "C".to_string()
        ]
    );
    assert!(m.expansion_unverified);
}

#[test]
fn derive_and_attr_macro() {
    let f = scan_source("t.rs", "#[derive(Debug, Clone)]\n#[tokio::main]\nstruct X;")
        .expect("parse ok");
    let mut derives: Vec<String> = f.derives.iter().map(|d| d.trait_name.clone()).collect();
    derives.sort();
    assert_eq!(derives, vec!["Clone".to_string(), "Debug".to_string()]);
    let attrs: Vec<String> = f
        .macros
        .iter()
        .filter(|m| matches!(m.kind, MacroEntryKind::AttrMacro))
        .map(|m| m.name.clone())
        .collect();
    assert_eq!(attrs, vec!["tokio::main".to_string()]);
}

#[test]
fn impl_for_vs_inherent_and_generics() {
    let f = scan_source(
        "t.rs",
        "impl Foo {}\nimpl<T: Send> Bar<T> for Baz<T> where T: Clone {}",
    )
    .expect("parse ok");
    let kinds: Vec<(Option<String>, Option<String>)> = f
        .impls
        .iter()
        .map(|i| (i.trait_name.clone(), i.type_name.clone()))
        .collect();
    assert!(kinds.contains(&(None, Some("Foo".to_string()))));
    assert!(kinds.contains(&(Some("Bar".to_string()), Some("Baz".to_string()))));
}

#[test]
fn rpit_not_counted_as_impl() {
    let f = scan_source(
        "t.rs",
        "fn make() -> impl Iterator<Item = u8> { core::iter::empty() }",
    )
    .expect("parse ok");
    assert!(f.impls.is_empty());
}

#[test]
fn doc_for_why_axis() {
    let f = scan_source(
        "t.rs",
        "/// Why this exists.\npub struct Documented;\npub struct Bare;",
    )
    .expect("parse ok");
    let documented = f
        .types
        .iter()
        .find(|t| t.name == "Documented")
        .expect("Documented type");
    let bare = f.types.iter().find(|t| t.name == "Bare").expect("Bare type");
    assert!(documented.doc.contains("Why this exists"));
    assert!(bare.doc.is_empty());
}
