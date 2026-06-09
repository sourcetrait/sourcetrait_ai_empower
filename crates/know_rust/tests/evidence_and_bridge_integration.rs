//! Integration tests for the broad-channels example-evidence fill and
//! the type_usage bridge's type-def gate.
//!
//! Evidence fill: derives / trait impls / macro calls occurring in
//! examples/ files earn curated_example_count credit (the_user
//! 2026-06-09 broad-channels design rule; previously only type_usage
//! factory calls and the AST synthesis paths carried example
//! evidence).
//!
//! Bridge gate: `type_usage:O::i` only aggregates a `structure:O`
//! pattern when O is a workspace TYPE def; mod / crate outers
//! (env::args-class) keep their implementation_functions entry but no
//! longer synthesize a structure pick.

use know_rust::*;
use std::collections::HashMap;
use std::path::Path;
use tempfile::TempDir;

fn write_tree(base: &Path, files: &HashMap<&str, String>) {
    for (rel, content) in files {
        let p = base.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("mkdir parent");
        }
        std::fs::write(&p, content).expect("write file");
    }
}

fn run_characterize(root: &Path) -> serde_json::Value {
    let out = root.join(".orientation");
    std::fs::create_dir_all(&out).expect("mkdir orientation");
    let calibration = Calibration::default();
    characterize(root, &out, &calibration).expect("characterize succeeds");
    let fp_text =
        std::fs::read_to_string(out.join("fingerprint.json")).expect("read fingerprint");
    serde_json::from_str(&fp_text).expect("parse fingerprint")
}

fn curated_for(fp: &serde_json::Value, key: &str) -> Option<u64> {
    fp.get("pattern_metrics")
        .and_then(|v| v.as_object())
        .and_then(|m| m.get(key))
        .and_then(|m| m.get("curated_example_count"))
        .and_then(|v| v.as_u64())
}

#[test]
fn example_evidence_for_derive_impl_and_macro() {
    // A derive, a trait impl, and a registration-macro call sitting in
    // an examples/ file each earn curated example evidence for their
    // pattern; the same constructs in src/ alone earn none.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let files: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from("[workspace]\nmembers=[\"lib\",\"app\"]\n"),
        ),
        (
            "lib/Cargo.toml",
            String::from(
                "[package]\nname=\"lib\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        (
            "lib/src/lib.rs",
            String::from(
                "pub trait Component {}\n\
                 pub trait Widget { fn render(&self); }\n\
                 pub trait Quiet {}\n\
                 #[macro_export]\nmacro_rules! reg { () => {}; }\n\
                 pub struct Core;\nimpl Core { pub fn new() -> Self { Core } }\n",
            ),
        ),
        (
            "lib/examples/demo.rs",
            String::from(
                "use lib::{Component, Widget};\n\
                 #[derive(Component)]\nstruct Demo;\n\
                 struct Shown;\nimpl Widget for Shown { fn render(&self) {} }\n\
                 fn main() { lib::reg!(); }\n",
            ),
        ),
        (
            "app/Cargo.toml",
            String::from(
                "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../lib\"}\n",
            ),
        ),
        (
            "app/src/lib.rs",
            String::from(
                "use lib::{Component, Widget, Quiet};\n\
                 #[derive(Component)]\npub struct D;\n\
                 impl Widget for D { fn render(&self) {} }\n\
                 pub struct Q;\nimpl Quiet for Q {}\n\
                 pub fn make() -> lib::Core { lib::Core::new() }\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let fp = run_characterize(root);

    assert_eq!(
        curated_for(&fp, "configuring:Component"),
        Some(1),
        "derive in examples/ earns configuring evidence"
    );
    assert_eq!(
        curated_for(&fp, "traits:Widget"),
        Some(1),
        "trait impl in examples/ earns traits evidence"
    );
    assert_eq!(
        curated_for(&fp, "utilities:reg"),
        Some(1),
        "macro call in examples/ earns utilities evidence"
    );
    assert_eq!(
        curated_for(&fp, "traits:Quiet"),
        Some(0),
        "src-only trait impl earns no example evidence"
    );
}

#[test]
fn bridge_requires_type_def_outer() {
    // `util::helper()` (mod outer) keeps its implementation_functions
    // entry but synthesizes no structure:util; `Core::new()` (type
    // outer) still aggregates structure:Core.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let files: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from("[workspace]\nmembers=[\"lib\",\"app\"]\n"),
        ),
        (
            "lib/Cargo.toml",
            String::from(
                "[package]\nname=\"lib\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        (
            "lib/src/lib.rs",
            String::from(
                "pub mod util { pub fn helper() {} }\n\
                 pub struct Core;\nimpl Core { pub fn new() -> Self { Core } }\n",
            ),
        ),
        (
            "app/Cargo.toml",
            String::from(
                "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../lib\"}\n",
            ),
        ),
        (
            "app/src/lib.rs",
            String::from(
                "pub fn run() -> lib::Core { lib::util::helper(); lib::Core::new() }\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let fp = run_characterize(root);
    let pm = fp
        .get("pattern_metrics")
        .and_then(|v| v.as_object())
        .expect("pattern_metrics object");

    assert!(
        pm.contains_key("implementation_functions:util::helper"),
        "mod-outer usage keeps its implementation_functions entry"
    );
    assert!(
        !pm.contains_key("structure:util"),
        "mod outer must not synthesize a structure pick; structure keys: {:?}",
        pm.keys().filter(|k| k.starts_with("structure:")).collect::<Vec<_>>()
    );
    assert!(
        pm.contains_key("structure:Core"),
        "type outer still aggregates structure:Core"
    );
}
