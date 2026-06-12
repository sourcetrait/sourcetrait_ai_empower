//! Integration test for the per-crate ballot resolution gate (dev
//! block 2, the_user ruling): the 5.5/5.6 tallies + clique ballots
//! pass per-site resolution like the key-level counting loop, so
//! same-named std/foreign usage never sweeps into workspace keys
//! (the nushell structure:File class: a workspace marker type
//! absorbing std::fs::File sites).

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
fn std_spelled_sites_never_ballot_workspace_keys() {
    // lib declares a marker type literally named `File` (the
    // nu-protocol id-marker shape) plus real API `Core`. app uses
    // std::fs::File (imported - resolves Std) and lib::Core
    // (imported - resolves Workspace). The ballot gate must keep
    // File rows OUT of app's per-crate sets while Core rows render.
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
                "pub struct File;\npub struct Core;\nimpl Core { pub fn new() -> Self { Core } }\n",
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
                "use std::fs::File;\nuse lib::Core;\n\
                 pub fn a() { let _ = File::open(\"x\"); let _ = Core::new(); }\n\
                 pub fn b() { let _ = File::create(\"y\"); let _ = Core::new(); }\n",
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
    let orient =
        std::fs::read_to_string(out.join("orientation.md")).expect("read orientation.md");

    // The per-crate tier (5.5 + 5.6): no File rows anywhere - the
    // std-spelled sites resolve Std and never ballot the workspace
    // marker; the marker itself has no credited usage.
    let per_crate_tier: String = orient
        .lines()
        .skip_while(|l| !l.starts_with("### 5.5"))
        .take_while(|l| !l.starts_with("## "))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !per_crate_tier.contains("`structure:File`")
            && !per_crate_tier.contains("File::open")
            && !per_crate_tier.contains("File::create"),
        "std-spelled File sites must not ballot the workspace marker; tier:\n{per_crate_tier}"
    );
    // Positive control: the real cross-crate usage still ballots.
    assert!(
        per_crate_tier.contains("`structure:Core`")
            || per_crate_tier.contains("`implementation_functions:Core::new`")
            || orient.contains("`structure:Core`"),
        "resolved workspace usage still renders; orientation:\n{}",
        orient.lines().take(40).collect::<Vec<_>>().join("\n")
    );
}
