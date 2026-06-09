//! Integration tests for R8 slice 3 grammar routing: enum variants
//! collapse into their enum's structure aggregate; constant-shaped
//! accesses are labels and route to the globals group.

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

#[test]
fn variants_collapse_constants_label_methods_stay() {
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
                "pub enum Filling { Text(String), Spaces(usize) }\n\
                 pub struct Dir3;\nimpl Dir3 { pub const Y: Dir3 = Dir3; pub fn new() -> Self { Dir3 } }\n",
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
                "use lib::{Filling, Dir3};\n\
                 pub fn take(_d: Dir3) {}\n\
                 pub fn run() {\n\
                     let _f = Filling::Text(String::new());\n\
                     let _g = Filling::Spaces(2);\n\
                     let _d = Dir3::new();\n\
                     take(Dir3::Y);\n\
                 }\n",
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
        .expect("pattern_metrics");

    assert!(
        pm.contains_key("structure:Filling"),
        "variant usage credits the enum's structure aggregate"
    );
    assert!(
        !pm.contains_key("implementation_functions:Filling::Text")
            && !pm.contains_key("implementation_functions:Filling::Spaces"),
        "no per-variant implementation_functions picks; impl-fn keys: {:?}",
        pm.keys()
            .filter(|k| k.starts_with("implementation_functions:Filling"))
            .collect::<Vec<_>>()
    );
    assert!(
        pm.contains_key("implementation_functions:Dir3::new"),
        "snake-case method picks unchanged"
    );
    assert!(
        pm.contains_key("globals:Dir3::Y"),
        "constant access routes to globals; globals keys: {:?}",
        pm.keys().filter(|k| k.starts_with("globals:")).collect::<Vec<_>>()
    );
    assert!(
        !pm.contains_key("implementation_functions:Dir3::Y")
            && !pm.keys().any(|k| k.starts_with("implementation_functions:_::Y")),
        "constants neither impl-fn picks nor `_::` families"
    );
}
