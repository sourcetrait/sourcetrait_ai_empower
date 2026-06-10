//! Integration tests for the documentation-kind profile knob: the
//! `author` default is byte-identical to pre-knob behavior, the
//! `consumer` profile drops the per-crate internals (no 5.4/5.5/5.6
//! sections) while widening the public set's cap, and an unknown
//! profile name fails loudly before any artifact is written.

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

/// A workspace with material across the tier: example-evidenced
/// structs (architecture + public), cross-crate-only usage (inter),
/// and per-crate intra material for the clique/intra/inner sections.
/// The lib crate deliberately carries NO pattern kinds of its own
/// (declarations only; the example exercises usage) so the shape
/// classifier lands `mixed` and emit takes the standard orientation
/// path - a demo-side trait impl would tip the container heuristic.
fn build_fixture(root: &Path) {
    let mut lib_src = String::from("pub trait Plug { fn go(&self); }\n");
    for i in 0..6 {
        lib_src.push_str(&format!(
            "pub struct S{i};\nimpl S{i} {{ pub fn new() -> Self {{ S{i} }} }}\n"
        ));
    }
    let mut demo = String::from("use lib::{S0, S1};\nfn main() {\n");
    demo.push_str("    let _ = S0::new();\n    let _ = S1::new();\n");
    demo.push_str("}\n");
    let mut app = String::from("use lib::*;\n");
    for i in 0..6 {
        app.push_str(&format!("pub fn u{i}() {{ let _ = S{i}::new(); }}\n"));
        app.push_str(&format!(
            "pub struct P{i};\nimpl Plug for P{i} {{ fn go(&self) {{}} }}\n"
        ));
    }
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
        ("lib/src/lib.rs", lib_src),
        ("lib/examples/demo.rs", demo),
        (
            "app/Cargo.toml",
            String::from(
                "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../lib\"}\n",
            ),
        ),
        ("app/src/lib.rs", app),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);
}

#[test]
fn author_profile_is_current_behavior_and_consumer_drops_internals() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    build_fixture(root);
    let out = root.join(".orientation");
    std::fs::create_dir_all(&out).expect("mkdir orientation");
    let calibration = Calibration::default();
    characterize(root, &out, &calibration).expect("characterize succeeds");
    let templates = Templates::new(None);

    emit(root, &out, &calibration, &templates, None, "author").expect("author emit");
    let author = std::fs::read_to_string(out.join("orientation.md")).expect("read author");
    assert!(
        author.contains("### 5.4") && author.contains("### 5.5") && author.contains("### 5.6"),
        "author keeps the full six-set tier"
    );

    emit(root, &out, &calibration, &templates, None, "consumer").expect("consumer emit");
    let consumer = std::fs::read_to_string(out.join("orientation.md")).expect("read consumer");
    assert!(
        consumer.contains("### 5.1") && consumer.contains("### 5.2")
            && consumer.contains("### 5.3"),
        "consumer keeps the workspace-wide consumer-facing sets"
    );
    assert!(
        !consumer.contains("### 5.4")
            && !consumer.contains("### 5.5")
            && !consumer.contains("### 5.6"),
        "consumer carries no clique / per-crate internals sections"
    );

    // The consumer bundle is a strict subset tier-wise, so its
    // forecastable surface must be smaller than the author bundle.
    assert!(
        consumer.len() < author.len(),
        "consumer bundle is smaller than author ({} vs {})",
        consumer.len(),
        author.len()
    );

    // Re-emitting author restores the full tier byte-identically
    // (the knob is a pure render-time lens over the same facts).
    emit(root, &out, &calibration, &templates, None, "author").expect("author re-emit");
    let author2 = std::fs::read_to_string(out.join("orientation.md")).expect("read author2");
    assert_eq!(author, author2, "author emits are reproducible");
}

#[test]
fn unknown_profile_fails_loudly() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    build_fixture(root);
    let out = root.join(".orientation");
    std::fs::create_dir_all(&out).expect("mkdir orientation");
    let calibration = Calibration::default();
    characterize(root, &out, &calibration).expect("characterize succeeds");
    let templates = Templates::new(None);
    let err = emit(root, &out, &calibration, &templates, None, "consmuer")
        .expect_err("typo'd profile must error");
    assert!(
        err.to_string().contains("unknown documentation profile `consmuer`"),
        "error names the unknown profile; got: {err}"
    );
}

#[test]
fn author_resolves_neutral_without_profile_sections() {
    // A custom calibration written before profiles existed (no
    // [profile.*] sections) must still emit under the default
    // profile: author falls back to the neutral scale.
    let toml_text = include_str!("../assets/calibration.toml");
    let stripped: String = toml_text
        .lines()
        .scan(false, |in_profile, line| {
            if line.starts_with("[profile.") {
                *in_profile = true;
            } else if line.starts_with('[') {
                *in_profile = false;
            }
            Some(if *in_profile { None } else { Some(line) })
        })
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
    let cal: Calibration = toml::from_str(&stripped).expect("profile-less toml parses");
    assert!(cal.profile.is_empty(), "fixture premise: no profiles");
    let scale = cal.resolve_profile("author").expect("author falls back to neutral");
    assert_eq!(scale.inner_crate, 1.0);
    assert!(cal.resolve_profile("consumer").is_err(), "others stay unknown");
}
