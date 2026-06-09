//! Reproducibility lock: emitting twice from the same characterize
//! outputs produces byte-identical orientation.md. The sampling
//! contract (working/08) says rerunning with the same inputs
//! reproduces the output deterministically; HashMap bucketing order
//! and unstable cap-boundary tie cuts used to randomize clique
//! elections and boundary picks between runs.

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
fn emit_twice_is_byte_identical() {
    // Enough patterns across several groups + crates that the old
    // HashMap-order randomness had room to differ: many same-score
    // ties at cap boundaries, multi-crate ballots for the clique.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();

    let mut lib_src = String::from("pub trait Plug { fn go(&self); }\n");
    for i in 0..12 {
        lib_src.push_str(&format!(
            "pub struct S{i};\nimpl S{i} {{ pub fn new() -> Self {{ S{i} }} }}\n"
        ));
    }
    let mut app_a = String::from("use lib::*;\n");
    let mut app_b = String::from("use lib::*;\n");
    for i in 0..12 {
        // Same usage count everywhere: maximal tie pressure.
        app_a.push_str(&format!("pub fn ua{i}() {{ let _ = S{i}::new(); }}\n"));
        app_b.push_str(&format!("pub fn ub{i}() {{ let _ = S{i}::new(); }}\n"));
        app_a.push_str(&format!(
            "pub struct PA{i};\nimpl Plug for PA{i} {{ fn go(&self) {{}} }}\n"
        ));
        app_b.push_str(&format!(
            "pub struct PB{i};\nimpl Plug for PB{i} {{ fn go(&self) {{}} }}\n"
        ));
    }
    let files: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from("[workspace]\nmembers=[\"lib\",\"appa\",\"appb\"]\n"),
        ),
        (
            "lib/Cargo.toml",
            String::from(
                "[package]\nname=\"lib\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        ("lib/src/lib.rs", lib_src),
        (
            "appa/Cargo.toml",
            String::from(
                "[package]\nname=\"appa\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../lib\"}\n",
            ),
        ),
        ("appa/src/lib.rs", app_a),
        (
            "appb/Cargo.toml",
            String::from(
                "[package]\nname=\"appb\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../lib\"}\n",
            ),
        ),
        ("appb/src/lib.rs", app_b),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let out = root.join(".orientation");
    std::fs::create_dir_all(&out).expect("mkdir orientation");
    let mut calibration = Calibration::default();
    // Tiny caps so the tie boundary actually cuts.
    calibration.picker.top_n_floor = 1;
    calibration.picker.sloc_divisor = 1_000_000;
    characterize(root, &out, &calibration).expect("characterize succeeds");
    let templates = Templates::new(None);

    emit(root, &out, &calibration, &templates).expect("first emit succeeds");
    let first = std::fs::read_to_string(out.join("orientation.md")).expect("read first");
    emit(root, &out, &calibration, &templates).expect("second emit succeeds");
    let second = std::fs::read_to_string(out.join("orientation.md")).expect("read second");

    assert_eq!(
        first, second,
        "two emits from identical inputs must be byte-identical"
    );
}
