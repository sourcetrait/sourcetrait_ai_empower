//! Integration test for R8 slice 5: patterns DEFINED in scaffolding
//! crates (examples / demo members) are pick-ineligible; usage FROM
//! scaffolding crates still credits real patterns.

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
