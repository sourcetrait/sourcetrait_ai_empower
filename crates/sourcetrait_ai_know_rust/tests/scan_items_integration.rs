//! Integration tests for `know_rust scan items`. Covers:
//! - workspace-level scan_items orchestration on a fixture workspace
//! - per-source-string walker behavior (port of test_orientation.py's
//!   six scan-level tests, lines 24-72)

use sourcetrait_ai_know_rust::*;
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
fn top_level_brace_use_emits_one_row_per_item() {
    // `use {a::b, c::d};` is N independent use paths, each with its
    // own ROOT - one flattened row would leak a `{`-prefixed
    // pseudo-root (the 5F `{crate` class). Nested groups keep the
    // shared prefix inline.
    let src = r#"
pub use {alpha::A, beta::B as Bee, crate::gamma::G};
use delta::{D1, D2};
"#;
    let f = scan_source("t.rs", src).expect("parse ok");
    let paths: Vec<&str> = f.uses.iter().map(|u| u.path.as_str()).collect();
    assert!(
        paths.contains(&"alpha::A")
            && paths.contains(&"beta::B as Bee")
            && paths.contains(&"crate::gamma::G"),
        "top-level group splits into per-item rows; got {paths:?}"
    );
    assert!(
        paths.contains(&"delta::{D1, D2}"),
        "nested group stays inline under its prefix; got {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.starts_with('{')),
        "no brace-prefixed pseudo-roots; got {paths:?}"
    );
    let reexports = f
        .uses
        .iter()
        .filter(|u| u.path.starts_with("alpha") || u.path.starts_with("beta") || u.path.contains("gamma"))
        .all(|u| u.reexport);
    assert!(reexports, "pub visibility carries to every split row");
}

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
fn macro_body_type_usage_keeps_path_root() {
    // A fully-qualified path inside a macro body keeps its written
    // root: writeln!(std::io::stdout(), ..) records name io::stdout
    // with qualifier "std" so resolution can route the site to std
    // instead of the crate-local fallback.
    let f = scan_source(
        "t.rs",
        "fn s() { writeln!(std::io::stdout(), \"x\").unwrap(); other!(custom::Thing::make()); }",
    )
    .expect("parse ok");
    let stdout_entry = f
        .type_usages
        .iter()
        .find(|t| t.name == "io::stdout")
        .expect("io::stdout captured from macro body");
    assert_eq!(
        stdout_entry.qualifier.as_deref(),
        Some("std"),
        "macro-body path root recovered"
    );
    let make_entry = f
        .type_usages
        .iter()
        .find(|t| t.name == "Thing::make")
        .expect("Thing::make captured from macro body");
    assert_eq!(
        make_entry.qualifier.as_deref(),
        Some("custom"),
        "non-std macro-body root recovered too"
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
fn macro_invocation_bare_pub_recovery_covers_items() {
    // Bare `pub` before trait / struct / type / fn / mod keywords in
    // a macro INVOCATION's tokens recovers the visibility the token
    // walk otherwise drops (bevy's define_label!{ pub trait
    // ScheduleLabel } class); macro_rules DEFINITION templates stay
    // blank (a template item is not a declaration until expanded).
    let f = scan_source(
        "t.rs",
        "define_label!{ pub trait ScheduleLabel {} pub struct Marker; pub type Alias = u8; }\nmacro_rules! tmpl { () => { pub trait TemplTrait {} }; }",
    )
    .expect("parse ok");
    let t = f
        .traits
        .iter()
        .find(|t| t.name == "ScheduleLabel")
        .expect("invocation trait recovered");
    assert_eq!(t.visibility, "pub", "invocation pub trait recovers vis");
    let s = f
        .types
        .iter()
        .find(|t| t.name == "Marker")
        .expect("invocation struct recovered");
    assert_eq!(s.visibility, "pub", "invocation pub struct recovers vis");
    let a = f
        .types
        .iter()
        .find(|t| t.name == "Alias")
        .expect("invocation type alias recovered");
    assert_eq!(a.visibility, "pub", "invocation pub type recovers vis");
    let tmpl = f
        .traits
        .iter()
        .find(|t| t.name == "TemplTrait")
        .expect("template trait still recorded");
    assert_eq!(tmpl.visibility, "", "macro_rules template stays blank");
}

#[test]
fn module_level_consts_statics_and_decl_fields() {
    // The labels stream: module-level const/static decls emit
    // ConstEntry rows with the inline mod chain; body-nested decls
    // carry None (decl-channel ineligible); assoc consts stay out of
    // the stream entirely. Type/trait decls carry the same
    // module_path/doc_hidden fields for the decl channel's types leg.
    let f = scan_source(
        "src/lib.rs",
        "pub const TOP: u8 = 1;\n\
         #[doc(hidden)]\npub static VEILED: u8 = 2;\n\
         pub mod colors { pub const RED: u8 = 3; }\n\
         fn body() { const NESTED: u8 = 4; let _ = NESTED; }\n\
         pub struct S1;\n\
         pub mod inner { pub type Alias1 = u8; pub trait T1 {} }\n\
         impl S1 { pub const ASSOC: u8 = 5; }\n",
    )
    .expect("parse ok");

    let top = f
        .consts
        .iter()
        .find(|c| c.name == "TOP")
        .expect("module-level const recorded");
    assert_eq!(top.module_path.as_deref(), Some(""), "file-top chain is empty");
    assert_eq!(top.visibility, "pub");
    assert!(!top.doc_hidden);

    let veiled = f
        .consts
        .iter()
        .find(|c| c.name == "VEILED")
        .expect("static recorded");
    assert!(veiled.doc_hidden, "doc(hidden) captured");
    assert!(
        matches!(veiled.kind, ConstEntryKind::Static),
        "static kind recorded"
    );

    let red = f
        .consts
        .iter()
        .find(|c| c.name == "RED")
        .expect("inline-mod const recorded");
    assert_eq!(
        red.module_path.as_deref(),
        Some("colors"),
        "inline mod chain recorded"
    );

    let nested = f
        .consts
        .iter()
        .find(|c| c.name == "NESTED")
        .expect("body-nested const still recorded");
    assert_eq!(nested.module_path, None, "body-nested decl is ineligible");

    assert!(
        !f.consts.iter().any(|c| c.name == "ASSOC"),
        "impl assoc consts stay out of the labels stream"
    );

    let s1 = f
        .types
        .iter()
        .find(|t| t.name == "S1")
        .expect("struct recorded");
    assert_eq!(s1.module_path.as_deref(), Some(""), "struct decl chain");
    let alias = f
        .types
        .iter()
        .find(|t| t.name == "Alias1")
        .expect("type alias recorded");
    assert_eq!(
        alias.module_path.as_deref(),
        Some("inner"),
        "type alias inline chain"
    );
    let t1 = f
        .traits
        .iter()
        .find(|t| t.name == "T1")
        .expect("trait recorded");
    assert_eq!(t1.module_path.as_deref(), Some("inner"), "trait inline chain");
}

#[test]
fn macro_invocation_top_level_decls_are_channel_eligible() {
    // The cfg_io_util class: a module-level macro INVOCATION's
    // stream-top-level fn carries the invocation site's chain
    // (decl-channel eligible) and a top-level `pub use` emits a
    // re-export UseEntry; nested-group fns and macro_rules
    // templates stay ineligible.
    let f = scan_source(
        "src/lib.rs",
        "mod cw { cfg_wrap!{ pub fn copy_like() {} } }\n\
         cfg_wrap!{ pub use cw::copy_like; }\n\
         cfg_wrap!{ if x { pub fn arm_fn() {} } }\n\
         macro_rules! tmpl { () => { pub fn tmpl_fn() {} }; }\n",
    )
    .expect("parse ok");
    let cl = f
        .fns
        .iter()
        .find(|x| x.name == "copy_like")
        .expect("macro-token fn recorded");
    assert_eq!(
        cl.module_path.as_deref(),
        Some("cw"),
        "invocation-site chain recorded"
    );
    assert_eq!(cl.visibility, "pub");
    let use_row = f
        .uses
        .iter()
        .find(|u| u.path == "cw::copy_like")
        .expect("macro-token pub use recorded; uses: {:?}");
    assert!(use_row.reexport, "bare pub recovered on the use");
    assert_eq!(use_row.module_path.as_deref(), Some(""));
    let arm = f
        .fns
        .iter()
        .find(|x| x.name == "arm_fn")
        .expect("nested-arm fn still recorded");
    assert_eq!(arm.module_path, None, "nested groups stay ineligible");
    let tf = f
        .fns
        .iter()
        .find(|x| x.name == "tmpl_fn")
        .expect("template fn recorded");
    assert_eq!(tf.module_path, None, "macro_rules templates stay ineligible");
}

#[test]
fn leading_colon_use_paths_keep_the_marker() {
    // The leading `::` is the explicit-external marker; the wire
    // must carry it (dropping it let local-module gates absorb
    // `pub use ::core::fmt::Write` beside a `mod core` shim).
    let f = scan_source(
        "t.rs",
        "pub use ::core::fmt::Write;\nuse ::alloc::vec::Vec;\nuse std::io::Read;\n",
    )
    .expect("parse ok");
    let paths: Vec<&str> = f.uses.iter().map(|u| u.path.as_str()).collect();
    assert!(
        paths.contains(&"::core::fmt::Write"),
        "explicit-external re-export keeps the prefix; paths: {paths:?}"
    );
    assert!(
        paths.contains(&"::alloc::vec::Vec"),
        "explicit-external import keeps the prefix"
    );
    assert!(
        paths.contains(&"std::io::Read"),
        "plain paths stay unprefixed"
    );
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
        .get(&Pattern::structure("Holder"))
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
        .get(&Pattern::structure("Outcome"))
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
        .get(&Pattern::structure("Mixed"))
        .expect("structure:Mixed carry list present");
    let names: Vec<&str> = carry.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"String"), "uppercase String kept");
    assert!(!names.contains(&"usize"), "lowercase usize filtered");
    assert!(!names.contains(&"bool"), "lowercase bool filtered");
    assert!(!names.contains(&"u8"), "lowercase u8 filtered");
}

#[test]
fn carry_configuring_records_derived_trait_name() {
    // A configured-via-attributes derive carries the derived trait name
    // under the configuring:<trait> key (the broadened/renamed group).
    let f = scan_source(
        "t.rs",
        "#[derive(Debug, Clone, PartialEq)]\npub struct Marker;",
    )
    .expect("parse ok");
    let debug_carry = f
        .carries
        .get(&Pattern::configuring("Debug"))
        .expect("configuring:Debug carry list present");
    let names: Vec<&str> = debug_carry.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"Debug"), "configuring:Debug carries Debug trait name");
    assert!(
        f.carries.contains_key(&Pattern::configuring("Clone")),
        "configuring:Clone present"
    );
    assert!(
        f.carries.contains_key(&Pattern::configuring("PartialEq")),
        "configuring:PartialEq present"
    );
}

#[test]
fn carry_impl_method_param_and_return_types() {
    let f = scan_source(
        "t.rs",
        "pub struct Thing;\nimpl Thing { pub fn build(input: String, dep: Dependency) -> OutputType { OutputType } }",
    )
    .expect("parse ok");
    let key = Pattern::impl_fn("Thing", "build");
    let carry = f
        .carries
        .get(&key)
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
        .get(&Pattern::traits("Composite"))
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
    let key = Pattern::trait_fn("Handler", "handle");
    let carry = f
        .carries
        .get(&key)
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
    let utility_keys: Vec<&Pattern> = f
        .carries
        .keys()
        .filter(|k| matches!(k.kind(), PickGroup::Utilities | PickGroup::Globals))
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
        .get(&Pattern::configuring("Component"))
        .expect("configuring:Component carry list present");
    let names: std::collections::HashSet<&str> = carry.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names.len(),
        1,
        "configuring:Component carry should dedup to 1 distinct trait name, got {:?}",
        carry
    );
    assert!(names.contains("Component"));
}

#[test]
fn carry_assoc_type_bounds() {
    // R2-expansion: `type X: Bound;` inside a trait carries the bound
    // under traits:<trait> (part of the trait's interface contract,
    // alongside supertype bounds; no assoc-type pick exists).
    let f = scan_source(
        "t.rs",
        "pub trait Container { type Item: Clone + IntoIterator; fn get(&self) -> Self::Item; }",
    )
    .expect("parse ok");
    let carry = f
        .carries
        .get(&Pattern::traits("Container"))
        .expect("traits:Container carry list present");
    let names: Vec<&str> = carry.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"Clone"), "Container carries assoc-type bound Clone");
    assert!(
        names.contains(&"IntoIterator"),
        "Container carries assoc-type bound IntoIterator"
    );
}

#[test]
fn carry_struct_generic_bounds() {
    // R2-expansion: struct generic-param + where-clause trait bounds
    // carry under structure:<name>.
    let f = scan_source(
        "t.rs",
        "pub struct Holder<T: Render, U> where U: Encode { pub item: T, pub other: U }",
    )
    .expect("parse ok");
    let carry = f
        .carries
        .get(&Pattern::structure("Holder"))
        .expect("structure:Holder carry list present");
    let names: Vec<&str> = carry.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"Render"), "Holder carries inline bound Render");
    assert!(names.contains(&"Encode"), "Holder carries where-clause bound Encode");
}

#[test]
fn carry_enum_generic_bounds() {
    // R2-expansion: enum generic-param trait bounds carry under
    // structure:<name>.
    let f = scan_source(
        "t.rs",
        "pub enum Either<L: Display, R> { Left(L), Right(R) }",
    )
    .expect("parse ok");
    let carry = f
        .carries
        .get(&Pattern::structure("Either"))
        .expect("structure:Either carry list present");
    let names: Vec<&str> = carry.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"Display"), "Either carries generic bound Display");
}

#[test]
fn carry_impl_generic_bounds() {
    // R2-expansion: impl generic-param + where-clause bounds carry under
    // structure:<impl_target> as reader context for the target type.
    let f = scan_source(
        "t.rs",
        "pub struct Foo<T>(pub T);\nimpl<T: Serialize> Foo<T> where T: Marker { pub fn go(&self) {} }",
    )
    .expect("parse ok");
    let carry = f
        .carries
        .get(&Pattern::structure("Foo"))
        .expect("structure:Foo carry list present");
    let names: Vec<&str> = carry.iter().map(|c| c.name.as_str()).collect();
    assert!(
        names.contains(&"Serialize"),
        "Foo impl carries inline bound Serialize"
    );
    assert!(
        names.contains(&"Marker"),
        "Foo impl carries where-clause bound Marker"
    );
}
