//! Integration tests for namespace-facade credit: sites spelled
//! through a crate-wholesale facade re-export (`pub use core_k;`,
//! `pub use core_k as ck;`, `pub use core_k::*;`, and transitive
//! chains) credit the DEFINING crate; external same-named imports
//! stay uncredited.

use sourcetrait_grammar_know_rust::*;
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

fn member(name: &str, deps: &str) -> String {
    format!(
        "[package]\nname=\"{}\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n{}",
        name, deps
    )
}

#[test]
fn namespace_facades_credit_the_defining_crate() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let files: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from(
                "[workspace]\nmembers=[\"core_k\",\"facade\",\"alias_f\",\"glob_f\",\"wrap\",\"app\"]\n",
            ),
        ),
        ("core_k/Cargo.toml", member("core_k", "")),
        (
            "core_k/src/lib.rs",
            String::from("pub struct Foo;\npub struct Bar;\npub fn util() {}\n"),
        ),
        (
            "facade/Cargo.toml",
            member("facade", "core_k={path=\"../core_k\"}\n"),
        ),
        (
            "facade/src/lib.rs",
            String::from("pub use core_k;\npub use core_k::Bar;\n"),
        ),
        (
            "alias_f/Cargo.toml",
            member("alias_f", "core_k={path=\"../core_k\"}\n"),
        ),
        (
            "alias_f/src/lib.rs",
            String::from("pub use core_k as ck;\n"),
        ),
        (
            "glob_f/Cargo.toml",
            member("glob_f", "core_k={path=\"../core_k\"}\n"),
        ),
        (
            "glob_f/src/lib.rs",
            String::from("pub use core_k::*;\n"),
        ),
        (
            "wrap/Cargo.toml",
            member("wrap", "facade={path=\"../facade\"}\n"),
        ),
        ("wrap/src/lib.rs", String::from("pub use facade;\n")),
        (
            "app/Cargo.toml",
            member(
                "app",
                "facade={path=\"../facade\"}\nalias_f={path=\"../alias_f\"}\nglob_f={path=\"../glob_f\"}\nwrap={path=\"../wrap\"}\n",
            ),
        ),
        (
            "app/src/lib.rs",
            String::from(
                "pub mod neg;\n\
                 use facade::core_k::Foo;\n\
                 use glob_f::Bar;\n\
                 pub fn a(_x: Foo) {}\n\
                 pub fn b(_x: alias_f::ck::Foo) {}\n\
                 pub fn c(_x: wrap::facade::core_k::Foo) {}\n\
                 pub fn d(_x: Bar) {}\n\
                 pub fn e() { facade::util(); }\n",
            ),
        ),
        (
            // Negative control: an external crate's Foo must not
            // credit the workspace pattern.
            "app/src/neg.rs",
            String::from("use ext_kit::Foo;\npub fn n(_x: Foo) {}\n"),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let out = root.join(".orientation");
    std::fs::create_dir_all(&out).expect("mkdir orientation");
    let calibration = Calibration::default();
    characterize(root, &out, &calibration).expect("characterize succeeds");
    let fp: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.join("fingerprint.json")).expect("read fingerprint"),
    )
    .expect("parse fingerprint");
    let pm = fp
        .get("pattern_metrics")
        .and_then(|v| v.as_object())
        .expect("pattern_metrics");

    let foo = pm.get("structure:Foo").expect("structure:Foo exists");
    assert_eq!(
        foo.get("defining_crate").and_then(|v| v.as_str()),
        Some("core_k")
    );
    assert_eq!(
        foo.get("inter_count").and_then(|v| v.as_u64()),
        Some(3),
        "direct ns (facade), aliased ns (alias_f::ck), and transitive ns (wrap) credit; the ext_kit site does not"
    );

    let bar = pm.get("structure:Bar").expect("structure:Bar exists");
    assert_eq!(
        bar.get("inter_count").and_then(|v| v.as_u64()),
        Some(1),
        "root-glob facade (pub use core_k::*) credits the glob_f-rooted site"
    );

    let util = pm
        .get("utilities:util")
        .expect("ns-facade-qualified free-fn call synthesizes utilities:util");
    assert_eq!(
        util.get("defining_crate").and_then(|v| v.as_str()),
        Some("core_k"),
        "free-fn redirect follows the namespace closure to the unique pub declaration"
    );
}
