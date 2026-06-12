//! Integration tests for the example-origin rule: items ORIGINATED
//! in example-dir files are never API (the_user) - never picked
//! (any set, any tier, including per-crate ballots) and never able
//! to serve demand. Usage FROM examples stays curated evidence (the
//! example-evidence alignment). The scaffolding-CRATE gate's
//! sibling: this covers example FILES belonging to a real package.

use sourcetrait_ai_know_rust::*;
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

/// lib declares real API in src/ (Core + RealWidget); its example
/// file declares its OWN trait + type (ExWidget / ExType), impls
/// them, and also impls the REAL trait (evidence for RealWidget).
/// app exercises the real API cross-crate.
fn build_fixture(root: &Path) -> std::path::PathBuf {
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
                "pub struct Core;\nimpl Core { pub fn new() -> Self { Core } }\n\
                 pub trait RealWidget { fn render(&self); }\n",
            ),
        ),
        (
            "lib/examples/demo.rs",
            String::from(
                "use lib::{Core, RealWidget};\n\
                 pub trait ExWidget { fn w(&self); }\n\
                 pub struct ExType;\n\
                 struct D;\nimpl ExWidget for D { fn w(&self) {} }\n\
                 impl RealWidget for ExType { fn render(&self) {} }\n\
                 fn main() { let _ = Core::new(); }\n",
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
                "use lib::{Core, RealWidget};\n\
                 pub struct W;\nimpl RealWidget for W { fn render(&self) {} }\n\
                 pub fn go() -> Core { Core::new() }\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);
    let out = root.join(".orientation");
    std::fs::create_dir_all(&out).expect("mkdir orientation");
    let calibration = Calibration::default();
    characterize(root, &out, &calibration).expect("characterize succeeds");
    let templates = Templates::new(None);
    emit(root, &out, &calibration, &templates, None, "author").expect("emit succeeds");
    out
}

#[test]
fn example_declared_items_are_never_picked() {
    let tmp = TempDir::new().expect("tempdir");
    let out = build_fixture(tmp.path());

    let fp: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.join("fingerprint.json")).expect("read fingerprint"),
    )
    .expect("parse fingerprint");
    let pm = fp
        .get("pattern_metrics")
        .and_then(|v| v.as_object())
        .expect("pattern_metrics");

    // The example-declared trait has no DEFINING side: its pm row
    // (seeded by the example-file impl fact) carries no
    // defining_crate, so the picker's origin gate drops it from
    // every set including the per-crate ballots.
    if let Some(m) = pm.get("traits:ExWidget") {
        assert!(
            m.get("defining_crate")
                .map(|v| v.is_null())
                .unwrap_or(true),
            "example-declared trait must not define; got {m}"
        );
    }
    assert!(
        !pm.contains_key("structure:ExType") || {
            pm.get("structure:ExType")
                .and_then(|m| m.get("defining_crate"))
                .map(|v| v.is_null())
                .unwrap_or(true)
        },
        "example-declared type must not define"
    );

    let orient =
        std::fs::read_to_string(out.join("orientation.md")).expect("read orientation.md");
    assert!(
        !orient.contains("`traits:ExWidget`") && !orient.contains("`structure:ExType`"),
        "example-declared items render in NO section"
    );

    // Positive controls: src-declared API is unaffected; the
    // example impl of RealWidget still earns curated evidence.
    let real = pm.get("traits:RealWidget").expect("real trait keyed");
    assert_eq!(
        real.get("defining_crate").and_then(|v| v.as_str()),
        Some("lib"),
        "src-declared trait defines normally"
    );
    assert_eq!(
        real.get("curated_example_count").and_then(|v| v.as_u64()),
        Some(1),
        "example impl of the REAL trait stays curated evidence"
    );
    assert!(
        orient.contains("`structure:Core`") || orient.contains("`implementation_functions:Core::new`"),
        "real API still picked"
    );
}

#[test]
fn example_decls_cannot_serve_demand() {
    let tmp = TempDir::new().expect("target tempdir");
    let out = build_fixture(tmp.path());

    // Consumer demands the example-declared ExType alongside real
    // API. ExType must read as a MISS with no vocabulary kind (an
    // example decl can never serve a demanded name).
    let ctmp = TempDir::new().expect("consumer tempdir");
    let cfiles: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from(
                "[package]\nname=\"consumer\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../t/lib\"}\n",
            ),
        ),
        (
            "src/lib.rs",
            String::from(
                "use lib::{Core, ExType};\npub fn go(_e: ExType) -> Core { Core::new() }\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(ctmp.path(), &cfiles);

    let out_path = ctmp.path().join("trace_out.json");
    let err = measure_demand(ctmp.path(), &out, Some(&out_path))
        .expect_err("the unservable ExType demand must gate");
    assert!(
        err.to_string().contains("miss"),
        "gate names the miss; got: {err}"
    );
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&out_path).expect("read --out json"),
    )
    .expect("parse --out json");
    let miss_names: Vec<&str> = report
        .pointer("/summary/misses")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.get("name").and_then(|v| v.as_str()))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        miss_names.contains(&"ExType"),
        "example-declared name is a MISS, not served; misses: {miss_names:?}"
    );
    let ex_kinds: Vec<&str> = report
        .pointer("/summary/misses")
        .and_then(|v| v.as_array())
        .and_then(|a| {
            a.iter()
                .find(|m| m.get("name").and_then(|v| v.as_str()) == Some("ExType"))
        })
        .and_then(|m| m.get("kinds").and_then(|v| v.as_array()))
        .map(|a| a.iter().filter_map(|k| k.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(
        ex_kinds,
        vec!["unknown"],
        "ExType is OUT of the target vocabulary (kind unknown)"
    );
    let hit_names: Vec<&str> = report
        .pointer("/hits")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.get("name").and_then(|v| v.as_str()))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        hit_names.contains(&"Core"),
        "real API still serves; hits: {hit_names:?}"
    );
}
