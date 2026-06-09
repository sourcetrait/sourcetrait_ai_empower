//! Integration test for R8 slice 4: the STV clique election sees the
//! full per-crate intra pools. Pre-slice, workspace-wide widening
//! stripped the ballots and small workspaces rendered "0 elected".

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
fn clique_elects_from_full_pools_even_when_workspace_wide_absorbs() {
    // The only intra material (traits:Plug, defined in lib, used by
    // app) is also inter-significant -> pre-slice the ballot was
    // stripped and clique rendered 0 elected.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let mut app_lib = String::from("use lib::Plug;\n");
    for i in 0..7 {
        app_lib.push_str(&format!(
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
        (
            "lib/src/lib.rs",
            String::from("pub trait Plug { fn go(&self); }\n"),
        ),
        (
            "app/Cargo.toml",
            String::from(
                "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../lib\"}\n",
            ),
        ),
        ("app/src/lib.rs", app_lib),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let out = root.join(".orientation");
    std::fs::create_dir_all(&out).expect("mkdir orientation");
    let calibration = Calibration::default();
    characterize(root, &out, &calibration).expect("characterize succeeds");
    let templates = Templates::new(None);
    emit(root, &out, &calibration, &templates).expect("emit succeeds");

    let orient =
        std::fs::read_to_string(out.join("orientation.md")).expect("read orientation.md");
    let clique_header = orient
        .lines()
        .find(|l| l.starts_with("### 5.4"))
        .expect("clique header present");
    assert!(
        !clique_header.contains("(0 elected"),
        "clique must elect from full pools; header: {}",
        clique_header
    );
    assert!(
        orient
            .lines()
            .skip_while(|l| !l.starts_with("### 5.4"))
            .take_while(|l| !l.starts_with("### 5.5"))
            .any(|l| l.contains("`traits:Plug`")),
        "the absorbed pattern still renders in the clique lens"
    );
}
