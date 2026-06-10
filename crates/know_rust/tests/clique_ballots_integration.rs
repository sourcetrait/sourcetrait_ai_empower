//! Integration test for R8 slice 4 (corrected): the STV clique
//! election sees the full per-crate intra pools, but the clique END
//! RESULT stays deduped against the workspace-wide sets (the_user:
//! "the answer needs to be yes"). On a workspace whose entire intra
//! material is workspace-wide-absorbed, the clique legitimately
//! renders 0 elected - what changed vs the ballot-stripping era is
//! that the election ranks real pools, so surplus-transfer winners
//! below absorbed picks survive on larger workspaces.

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
fn clique_result_is_deduped_against_workspace_wide_sets() {
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
    emit(root, &out, &calibration, &templates, None, "author").expect("emit succeeds");

    let orient =
        std::fs::read_to_string(out.join("orientation.md")).expect("read orientation.md");

    // traits:Plug sits in the inter set (workspace-wide).
    let inter_section: String = orient
        .lines()
        .skip_while(|l| !l.starts_with("### 5.3"))
        .take_while(|l| !l.starts_with("### 5.4"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        inter_section.contains("`traits:Plug`"),
        "fixture premise: traits:Plug is workspace-wide"
    );

    // The clique end result must not repeat it (deduped); with the
    // entire intra pool absorbed, this fixture's clique is empty.
    let clique_section: String = orient
        .lines()
        .skip_while(|l| !l.starts_with("### 5.4"))
        .take_while(|l| !l.starts_with("### 5.5"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !clique_section.contains("`traits:Plug`"),
        "clique end result stays deduped against workspace-wide sets"
    );
}
