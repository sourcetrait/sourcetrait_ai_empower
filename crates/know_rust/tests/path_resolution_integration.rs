//! Integration tests for item path resolution (R8 slice 2): the
//! workspace-origin rule holds at usage sites via per-file import
//! resolution. No separate stdlib rule - std-resolved identifiers
//! are simply never candidates.

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
    serde_json::from_str(
        &std::fs::read_to_string(out.join("fingerprint.json")).expect("read fingerprint"),
    )
    .expect("parse fingerprint")
}

fn metric<'a>(fp: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    fp.get("pattern_metrics")
        .and_then(|v| v.as_object())
        .and_then(|m| m.get(key))
}

#[test]
fn prelude_alias_does_not_absorb_std_usage() {
    // lib aliases std Result under the same name (tokio pattern). app
    // fn sigs use bare prelude Result twice and the workspace alias
    // once (via use lib::Result). Only the imported site credits.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let files: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from("[workspace]\nmembers=[\"lib\",\"app\",\"app2\"]\n"),
        ),
        (
            "lib/Cargo.toml",
            String::from(
                "[package]\nname=\"lib\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        (
            "lib/src/lib.rs",
            String::from("pub type Result<T> = std::result::Result<T, ()>;\npub struct Core;\n"),
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
                "use lib::Result;\npub fn with_alias(_c: lib::Core) -> Result<u8> { Ok(1) }\n",
            ),
        ),
        (
            "app2/Cargo.toml",
            String::from(
                "[package]\nname=\"app2\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../lib\"}\n",
            ),
        ),
        (
            "app2/src/lib.rs",
            String::from(
                "pub fn plain_a() -> Result<u8, ()> { Ok(1) }\npub fn plain_b() -> Result<u8, ()> { Ok(2) }\npub fn touch(_c: lib::Core) {}\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let fp = run_characterize(root);
    let m = metric(&fp, "structure:Result").expect("structure:Result exists (one credited site)");
    let inter = m.get("inter_count").and_then(|v| v.as_u64()).unwrap_or(99);
    let intra = m.get("intra_count").and_then(|v| v.as_u64()).unwrap_or(99);
    // The load-bearing property: cross-crate absorption is dead -
    // only the use-lib::Result site credits inter. Known precision
    // limit: the usages stream strips path qualifiers, so a
    // fully-qualified `std::result::Result` INSIDE the declaring
    // crate (here: the alias decl's own RHS) shadow-resolves to
    // SelfCrate and credits intra. Intra does not feed the
    // workspace-wide sets.
    assert_eq!(
        inter, 1,
        "only the use-lib::Result site credits inter; prelude sites resolve to std (got inter {})",
        inter
    );
    assert!(
        intra <= 1,
        "at most the declaring crate's own qualified-path residual (got intra {})",
        intra
    );
}

#[test]
fn std_mod_calls_are_not_picks_but_workspace_mod_fns_are() {
    // `use std::io;` + io::stdin() must produce no pattern; `use
    // lib::util;` + util::helper() keeps the workspace mod-fn pick.
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
            String::from("pub mod util { pub fn helper() {} }\npub mod io { pub fn noop() {} }\n"),
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
                "use std::io;\nuse lib::util;\npub fn run() {\n    let _h = io::stdin();\n    util::helper();\n}\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let fp = run_characterize(root);
    assert!(
        metric(&fp, "implementation_functions:io::stdin")
            .map(|m| m.get("defining_crate").map(|v| v.is_null()).unwrap_or(true))
            .unwrap_or(true),
        "std-resolved io::stdin must not be workspace-attributed"
    );
    let util = metric(&fp, "implementation_functions:util::helper")
        .expect("workspace mod-fn pattern kept");
    assert_eq!(
        util.get("defining_crate").and_then(|v| v.as_str()),
        Some("lib"),
        "util::helper attributes to lib via the import"
    );
}

#[test]
fn external_import_does_not_credit_workspace_trait_of_same_name() {
    // Two impls of `Widget`: one file imports the workspace trait,
    // the other imports an external crate's trait of the same name.
    // Only the workspace-imported impl credits traits:Widget.
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
            String::from("pub trait Widget { fn render(&self); }\n"),
        ),
        (
            "app/Cargo.toml",
            String::from(
                "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../lib\"}\n",
            ),
        ),
        (
            "app/src/ours.rs",
            String::from(
                "use lib::Widget;\npub struct A;\nimpl Widget for A { fn render(&self) {} }\n",
            ),
        ),
        (
            "app/src/theirs.rs",
            String::from(
                "use other_gui::Widget;\npub struct B;\nimpl Widget for B { fn render(&self) {} }\n",
            ),
        ),
        (
            "app/src/lib.rs",
            String::from("pub mod ours;\npub mod theirs;\n"),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let fp = run_characterize(root);
    let m = metric(&fp, "traits:Widget").expect("traits:Widget exists");
    let inter = m.get("inter_count").and_then(|v| v.as_u64()).unwrap_or(99);
    let intra = m.get("intra_count").and_then(|v| v.as_u64()).unwrap_or(99);
    assert_eq!(
        intra + inter,
        1,
        "only the lib-imported impl credits; the external-imported impl does not (got intra {} inter {})",
        intra,
        inter
    );
}
