//! Integration tests for the bindings->identity vocabulary: source
//! roots reach crates through `[lib]` rename bindings and
//! per-consuming-crate dependency renames; package-name-only
//! resolution (the prior vocabulary) would have sent every such site
//! to External and lost it.

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

#[test]
fn lib_and_dep_renames_resolve_to_the_package() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let files: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from("[workspace]\nmembers=[\"render-kit\",\"app1\",\"app2\"]\n"),
        ),
        (
            "render-kit/Cargo.toml",
            String::from(
                "[package]\nname=\"render-kit\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[lib]\nname=\"rkit\"\n[dependencies]\n",
            ),
        ),
        (
            "render-kit/src/lib.rs",
            String::from(
                "pub trait Widget { fn render(&self); }\npub struct Frame;\npub fn boot() {}\n",
            ),
        ),
        (
            "app1/Cargo.toml",
            String::from(
                "[package]\nname=\"app1\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nrender-kit={path=\"../render-kit\"}\n",
            ),
        ),
        (
            // The LIB binding: imports and qualified roots say `rkit`,
            // the package is `render-kit`.
            "app1/src/lib.rs",
            String::from(
                "use rkit::{Widget, Frame};\n\
                 pub struct A;\nimpl Widget for A { fn render(&self) {} }\n\
                 pub fn draw(_f: Frame) {}\n\
                 pub fn start() { rkit::boot(); }\n",
            ),
        ),
        (
            "app2/Cargo.toml",
            String::from(
                "[package]\nname=\"app2\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nrk={package=\"render-kit\", path=\"../render-kit\"}\n",
            ),
        ),
        (
            // The DEP-RENAME binding: this crate's manifest binds `rk`
            // to package render-kit.
            "app2/src/lib.rs",
            String::from(
                "pub mod ours;\npub mod theirs;\n",
            ),
        ),
        (
            "app2/src/ours.rs",
            String::from(
                "use rk::Widget;\npub struct B;\nimpl Widget for B { fn render(&self) {} }\n",
            ),
        ),
        (
            // Negative control: an external crate's same-named trait
            // must not credit the workspace pattern.
            "app2/src/theirs.rs",
            String::from(
                "use ext_gui::Widget;\npub struct C;\nimpl Widget for C { fn render(&self) {} }\n",
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
    let fp: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.join("fingerprint.json")).expect("read fingerprint"),
    )
    .expect("parse fingerprint");
    let pm = fp
        .get("pattern_metrics")
        .and_then(|v| v.as_object())
        .expect("pattern_metrics");

    let widget = pm.get("traits:Widget").expect("traits:Widget exists");
    assert_eq!(
        widget.get("defining_crate").and_then(|v| v.as_str()),
        Some("render-kit")
    );
    let total = widget.get("intra_count").and_then(|v| v.as_u64()).unwrap_or(99)
        + widget.get("inter_count").and_then(|v| v.as_u64()).unwrap_or(99);
    assert_eq!(
        total, 2,
        "lib-binding impl (app1) + dep-rename impl (app2) credit; the ext_gui impl does not"
    );

    let frame = pm.get("structure:Frame").expect("structure:Frame exists");
    assert!(
        frame.get("inter_count").and_then(|v| v.as_u64()).unwrap_or(0) >= 1,
        "lib-binding import resolves the sig usage to render-kit"
    );

    let boot = pm
        .get("utilities:boot")
        .expect("lib-binding-qualified free-fn call synthesizes utilities:boot");
    assert_eq!(
        boot.get("defining_crate").and_then(|v| v.as_str()),
        Some("render-kit"),
        "free-fn qualifier resolves through the lib binding"
    );
}
