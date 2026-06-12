//! Integration test for deterministic workspace-shape signals:
//! identical-input characterize runs must produce byte-identical
//! fingerprints (the sampling contract), including the central_crate
//! and per-crate top-kind elections, which previously rode HashMap
//! iteration order and flipped on ties (bevy_ecs vs bevy_reflect at
//! 44 dependents flipping central_crate; a libcosmic member's kind
//! tie drifting distinct_top_kinds / dispersion).

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
fn tied_elections_are_deterministic_and_fingerprint_reproduces() {
    // alpha and beta tie at one dependent each (central-crate tie);
    // app carries one trait_impl and one derive (per-crate kind tie).
    // Both elections must land deterministically: dependents desc
    // then crate name asc -> alpha; the kind tie is covered by the
    // byte-identity of two fresh characterize runs.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let files: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from("[workspace]\nmembers=[\"alpha\",\"beta\",\"app\"]\n"),
        ),
        (
            "alpha/Cargo.toml",
            String::from(
                "[package]\nname=\"alpha\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        (
            "alpha/src/lib.rs",
            String::from("pub trait T { fn go(&self); }\npub struct A;\n"),
        ),
        (
            "beta/Cargo.toml",
            String::from(
                "[package]\nname=\"beta\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        (
            "beta/src/lib.rs",
            String::from("pub trait D {}\npub struct B;\n"),
        ),
        (
            "app/Cargo.toml",
            String::from(
                "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nalpha={path=\"../alpha\"}\nbeta={path=\"../beta\"}\n",
            ),
        ),
        (
            "app/src/lib.rs",
            String::from(
                "use alpha::T;\nuse beta::D;\n#[derive(D)]\npub struct X;\nimpl T for X { fn go(&self) {} }\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let calibration = Calibration::default();
    let out1 = root.join(".orientation_a");
    let out2 = root.join(".orientation_b");
    std::fs::create_dir_all(&out1).expect("mkdir out1");
    std::fs::create_dir_all(&out2).expect("mkdir out2");
    characterize(root, &out1, &calibration).expect("first characterize succeeds");
    characterize(root, &out2, &calibration).expect("second characterize succeeds");

    let fp1 = std::fs::read_to_string(out1.join("fingerprint.json")).expect("read fp1");
    let fp2 = std::fs::read_to_string(out2.join("fingerprint.json")).expect("read fp2");
    assert_eq!(
        fp1, fp2,
        "two characterize runs from identical inputs must produce byte-identical fingerprints"
    );

    let fp: serde_json::Value = serde_json::from_str(&fp1).expect("parse fingerprint");
    assert_eq!(
        fp.pointer("/workspace_shape/signals/central_crate")
            .and_then(|v| v.as_str()),
        Some("alpha"),
        "tied central-crate election resolves dependents desc, name asc"
    );
}
