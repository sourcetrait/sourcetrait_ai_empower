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

// --- Carry extraction tests (R2 + R2-expansion + R3 + dedup
// behaviour locked in per the picks-data refactor at
// notes/know_rust/tasks/picks-data-model-refactor.md). The walker
// surfaces one-hop transitive dependencies under each picked
// pattern's Pattern::Display key (`<group_wire>:<name>`). ---

#[test]
fn carry_struct_field_types() {
    // type_carry_names filters to uppercase-starting identifiers
    // (primitives like usize / bool / u8 are excluded by design;
    // see src/scan/items/helpers.rs::type_carry_names).
    let f = scan_source(
        "t.rs",
        "pub struct Holder { pub name: String, pub vec: Vec<u8>, pub kid: ChildType }",
    )
    .expect("parse ok");
    let carry = f
        .carries
        .get("structure:Holder")
        .expect("structure:Holder carry list present");
    let names: Vec<&str> = carry.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"String"), "Holder carries String");
    assert!(names.contains(&"Vec"), "Holder carries Vec");
    assert!(names.contains(&"ChildType"), "Holder carries ChildType");
}

#[test]
fn carry_enum_variant_payloads() {
    let f = scan_source(
        "t.rs",
        "pub enum Outcome { Success(String), Failure(ErrorBox, Detail), None }",
    )
    .expect("parse ok");
    let carry = f
        .carries
        .get("structure:Outcome")
        .expect("structure:Outcome carry list present");
    let names: Vec<&str> = carry.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"String"), "Outcome carries String (Success payload)");
    assert!(names.contains(&"ErrorBox"), "Outcome carries ErrorBox (Failure payload 0)");
    assert!(names.contains(&"Detail"), "Outcome carries Detail (Failure payload 1)");
}

#[test]
fn carry_filters_lowercase_primitives() {
    // Explicit coverage: primitives (u8 / bool / usize) and lifetime
    // params are excluded from carry.
    let f = scan_source(
        "t.rs",
        "pub struct Mixed<'a> { pub name: String, pub n: usize, pub flag: bool, pub bytes: &'a [u8] }",
    )
    .expect("parse ok");
    let carry = f
        .carries
        .get("structure:Mixed")
        .expect("structure:Mixed carry list present");
    let names: Vec<&str> = carry.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"String"), "uppercase String kept");
    assert!(!names.contains(&"usize"), "lowercase usize filtered");
    assert!(!names.contains(&"bool"), "lowercase bool filtered");
    assert!(!names.contains(&"u8"), "lowercase u8 filtered");
}

#[test]
fn carry_derives_records_trait_name() {
    let f = scan_source(
        "t.rs",
        "#[derive(Debug, Clone, PartialEq)]\npub struct Marker;",
    )
    .expect("parse ok");
    let debug_carry = f
        .carries
        .get("derives:Debug")
        .expect("derives:Debug carry list present");
    let names: Vec<&str> = debug_carry.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"Debug"), "derives:Debug carries Debug trait name");
    assert!(
        f.carries.contains_key("derives:Clone"),
        "derives:Clone present"
    );
    assert!(
        f.carries.contains_key("derives:PartialEq"),
        "derives:PartialEq present"
    );
}

#[test]
fn carry_impl_method_param_and_return_types() {
    let f = scan_source(
        "t.rs",
        "pub struct Thing;\nimpl Thing { pub fn build(input: String, dep: Dependency) -> OutputType { OutputType } }",
    )
    .expect("parse ok");
    let key = "implementation_functions:Thing::build";
    let carry = f
        .carries
        .get(key)
        .unwrap_or_else(|| panic!("{key} carry list present; got keys: {:?}", f.carries.keys().collect::<Vec<_>>()));
    let names: Vec<&str> = carry.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"String"), "Thing::build carries String");
    assert!(names.contains(&"Dependency"), "Thing::build carries Dependency");
    assert!(names.contains(&"OutputType"), "Thing::build carries OutputType (return)");
}

#[test]
fn carry_trait_supertype_bounds() {
    let f = scan_source(
        "t.rs",
        "pub trait Send {}\npub trait Sync {}\npub trait Composite: Send + Sync {}",
    )
    .expect("parse ok");
    let carry = f
        .carries
        .get("traits:Composite")
        .expect("traits:Composite carry list present");
    let names: Vec<&str> = carry.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"Send"), "Composite carries supertype Send");
    assert!(names.contains(&"Sync"), "Composite carries supertype Sync");
}

#[test]
fn carry_trait_method_sig_types() {
    let f = scan_source(
        "t.rs",
        "pub trait Handler { fn handle(&self, input: String) -> ResponseType; }",
    )
    .expect("parse ok");
    let key = "trait_functions:Handler::handle";
    let carry = f
        .carries
        .get(key)
        .unwrap_or_else(|| panic!("{key} carry list present; got keys: {:?}", f.carries.keys().collect::<Vec<_>>()));
    let names: Vec<&str> = carry.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"String"), "Handler::handle carries String");
    assert!(names.contains(&"ResponseType"), "Handler::handle carries ResponseType");
}

#[test]
fn carry_no_carry_for_utilities_globals() {
    // Utilities (free fns + macros) and globals (consts + statics) do
    // not have carry per the picks-data model. The walker should not
    // emit any utilities:<name> or globals:<name> keys.
    let f = scan_source(
        "t.rs",
        "pub fn standalone(x: String) -> usize { 0 }\npub const TOP: usize = 42;\npub static LABEL: &str = \"x\";\nmacro_rules! noisy { () => {}; }",
    )
    .expect("parse ok");
    let utility_keys: Vec<&String> = f
        .carries
        .keys()
        .filter(|k| k.starts_with("utilities:") || k.starts_with("globals:"))
        .collect();
    assert!(
        utility_keys.is_empty(),
        "no carry expected for utilities/globals; got {:?}",
        utility_keys
    );
}

#[test]
fn carry_workspace_dedup_by_name() {
    // The workspace-level carry merge dedups by name per pattern key
    // (workspace.rs::scan_workspace finalization). Two derives of
    // the same trait across different structs should collapse to one
    // carry entry.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let files: std::collections::HashMap<&str, &str> = [
        (
            "Cargo.toml",
            "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
        ),
        (
            "src/lib.rs",
            "pub trait Component {}\n#[derive(Component)]\npub struct A;\n#[derive(Component)]\npub struct B;\n#[derive(Component)]\npub struct C;\n",
        ),
    ]
    .iter()
    .cloned()
    .collect();
    for (rel, content) in &files {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("mkdir parent");
        }
        std::fs::write(&p, content).expect("write file");
    }
    let out = TempDir::new().expect("scan out tempdir");
    scan_items(root, out.path()).expect("scan_items succeeds");
    let items_path = out.path().join("know_rust_items.json");
    let json = std::fs::read_to_string(&items_path).expect("read items json");
    let facts: ItemFacts = serde_json::from_str(&json).expect("parse ItemFacts");
    let carry = facts
        .carries
        .get("derives:Component")
        .expect("derives:Component carry list present");
    let names: std::collections::HashSet<&str> = carry.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names.len(),
        1,
        "derives:Component carry should dedup to 1 distinct trait name, got {:?}",
        carry
    );
    assert!(names.contains("Component"));
}
