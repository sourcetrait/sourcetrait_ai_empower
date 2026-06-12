//! Integration test for R8 slice 5: patterns DEFINED in scaffolding
//! crates (examples / demo members) are pick-ineligible; usage FROM
//! scaffolding crates still credits real patterns.

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

#[test]
fn scaffolding_defined_patterns_are_not_picked() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let files: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from("[workspace]\nmembers=[\"lib\",\"app\",\"examples/demo\"]\n"),
        ),
        (
            "lib/Cargo.toml",
            String::from(
                "[package]\nname=\"lib\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        (
            "lib/src/lib.rs",
            String::from("pub struct Core;\nimpl Core { pub fn new() -> Self { Core } }\n"),
        ),
        (
            "examples/demo/Cargo.toml",
            String::from(
                "[package]\nname=\"demo\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../../lib\"}\n",
            ),
        ),
        (
            "examples/demo/src/lib.rs",
            String::from(
                "pub struct DemoType;\npub fn show() -> lib::Core { lib::Core::new() }\n",
            ),
        ),
        (
            "app/Cargo.toml",
            String::from(
                "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../lib\"}\ndemo={path=\"../examples/demo\"}\n",
            ),
        ),
        (
            "app/src/lib.rs",
            String::from(
                "use demo::DemoType;\nuse lib::Core;\npub fn run(_d: DemoType) -> Core { lib::Core::new() }\n",
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

    let fp: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.join("fingerprint.json")).expect("read fingerprint"),
    )
    .expect("parse fingerprint");
    let scaffolding = fp
        .get("workspace_use_classification")
        .and_then(|v| v.get("scaffolding_crates"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();
    assert!(
        scaffolding.contains(&"demo"),
        "fixture demo crate classifies as scaffolding; got {:?}",
        scaffolding
    );

    // The decl channel never MINTS scaffolding-crate decls (the
    // unit-9 rustls_test class): demo's pub fn `show` gets no key.
    // The demo member also sits under examples/, so the
    // example-origin rule strips its decls of any DEFINING side:
    // structure:DemoType either drops from pm entirely or carries
    // a null defining_crate (the picker's origin gate drops both).
    let pm = fp
        .get("pattern_metrics")
        .and_then(|v| v.as_object())
        .expect("pattern_metrics");
    assert!(
        !pm.keys().any(|k| k.ends_with("::show")),
        "scaffolding fn decls never mint; keys: {:?}",
        pm.keys().filter(|k| k.contains("show")).collect::<Vec<_>>()
    );
    if let Some(demo_type) = pm.get("structure:DemoType") {
        assert!(
            demo_type
                .get("defining_crate")
                .map(|v| v.is_null())
                .unwrap_or(true),
            "example-origin: DemoType must not define; got {demo_type}"
        );
    }
    let orient =
        std::fs::read_to_string(out.join("orientation.md")).expect("read orientation.md");
    assert!(
        !orient.contains("`structure:DemoType`"),
        "scaffolding-defined type must not be picked"
    );
    assert!(
        orient.contains("`structure:Core`")
            || orient.contains("`implementation_functions:Core::new`"),
        "real pattern still picked (scaffolding USAGE credits it)"
    );
}
