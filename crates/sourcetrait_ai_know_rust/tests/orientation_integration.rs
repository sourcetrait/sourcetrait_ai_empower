//! Integration tests for the characterize + emit pipeline. Ports
//! test_orientation.py's four workspace-shape tests (lines 90-169).

use sourcetrait_ai_know_rust::*;
use std::collections::HashMap;
use std::path::Path;
use tempfile::TempDir;

fn write_tree(base: &Path, files: &HashMap<&str, &str>) {
    for (rel, content) in files {
        let p = base.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("mkdir parent");
        }
        std::fs::write(&p, content).expect("write file");
    }
}

fn run_characterize(root: &Path) -> (serde_json::Value, std::path::PathBuf) {
    let out = root.join(".orientation");
    std::fs::create_dir_all(&out).expect("mkdir orientation");
    let calibration = Calibration::default();
    characterize(root, &out, &calibration).expect("characterize succeeds");
    let fp_text =
        std::fs::read_to_string(out.join("fingerprint.json")).expect("read fingerprint");
    let fp: serde_json::Value = serde_json::from_str(&fp_text).expect("parse fingerprint");
    (fp, out)
}

#[test]
fn nushell_shape_single_dominant() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let cmds: String = (0..25)
        .map(|i| format!("pub struct C{};\nimpl Command for C{} {{ fn run(&self) {{}} }}", i, i))
        .collect::<Vec<_>>()
        .join("\n");
    let c_lib = format!(
        "use p::Command;\n{}\nfn reg(){{ bind_command!(C0,C1,C2); }}",
        cmds
    );
    let files: HashMap<&str, &str> = [
        ("Cargo.toml", "[workspace]\nmembers=[\"p\",\"c\"]\n"),
        ("p/Cargo.toml", "[package]\nname=\"p\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n"),
        ("p/src/lib.rs", "pub trait Command { fn run(&self); }\npub struct Value;"),
        ("c/Cargo.toml", "[package]\nname=\"c\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\np={path=\"../p\"}\n"),
        ("c/src/lib.rs", c_lib.as_str()),
    ]
    .iter()
    .cloned()
    .collect();
    write_tree(root, &files);

    let (fp, _out) = run_characterize(root);
    let top_pattern = fp
        .get("pattern_histogram")
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("pattern"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(top_pattern, "trait_impl:Command");
    let mode = fp
        .get("selection")
        .and_then(|v| v.get("histogram_mode"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(mode, "single_dominant");
    let n_components = fp.get("n_components").and_then(|v| v.as_u64()).unwrap_or(0);
    assert_eq!(n_components, 1);
}

#[test]
fn bevy_shape_derive_coequal() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let comps: String = (0..20)
        .map(|i| format!("#[derive(Component)]\npub struct Pos{};", i))
        .collect::<Vec<_>>()
        .join("\n");
    let systems: String = (0..20)
        .map(|i| format!("pub fn system_{}(q: Query) {{}}", i))
        .collect::<Vec<_>>()
        .join("\n");
    let game_lib = format!("use ecs::Component;\n{}\n{}", comps, systems);
    let files: HashMap<&str, &str> = [
        ("Cargo.toml", "[workspace]\nmembers=[\"ecs\",\"game\"]\n"),
        (
            "ecs/Cargo.toml",
            "[package]\nname=\"ecs\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
        ),
        ("ecs/src/lib.rs", "pub trait Component {}\npub struct Query;"),
        (
            "game/Cargo.toml",
            "[package]\nname=\"game\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\necs={path=\"../ecs\"}\n",
        ),
        ("game/src/lib.rs", game_lib.as_str()),
    ]
    .iter()
    .cloned()
    .collect();
    write_tree(root, &files);

    let (fp, _out) = run_characterize(root);
    let derive_count = fp
        .get("pattern_by_kind")
        .and_then(|v| v.get("derive"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(derive_count >= 20, "derive count >= 20, got {}", derive_count);
    let top = fp
        .get("pattern_histogram")
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("pattern"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        top.starts_with("derive:") || top.starts_with("fn_table:"),
        "dominant pattern should be derive or fn_table, got {}",
        top
    );
}

#[test]
fn regional_multiworkspace() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let files: HashMap<&str, &str> = [
        (
            "kernel/Cargo.toml",
            "[workspace]\nmembers=[\".\"]\n[package]\nname=\"kernel\"\nversion=\"0.0.1\"\nedition=\"2021\"\n",
        ),
        (
            "kernel/src/lib.rs",
            "#![no_std]\nextern \"C\" { fn syscall(n: usize); }\n",
        ),
        ("user/Cargo.toml", "[workspace]\nmembers=[\"a\"]\n"),
        (
            "user/a/Cargo.toml",
            "[package]\nname=\"a\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
        ),
        ("user/a/src/lib.rs", "pub trait T{}\npub struct S;\nimpl T for S{}"),
    ]
    .iter()
    .cloned()
    .collect();
    write_tree(root, &files);

    let (fp, _out) = run_characterize(root);
    let mode = fp
        .get("selection")
        .and_then(|v| v.get("mode"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(mode, "regional", "selection.mode should be regional");
    let n_components = fp.get("n_components").and_then(|v| v.as_u64()).unwrap_or(0);
    assert!(n_components >= 2, "n_components >= 2, got {}", n_components);
    let notes_empty: Vec<serde_json::Value> = Vec::new();
    let notes = fp
        .get("selection")
        .and_then(|v| v.get("notes"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or(notes_empty);
    let has_signal_note = notes
        .iter()
        .filter_map(|v| v.as_str())
        .any(|s| s.contains("structural signals"));
    assert!(has_signal_note, "notes should contain a 'structural signals' entry");
}

#[test]
fn emit_two_files_and_spans() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let files: HashMap<&str, &str> = [
        (
            "Cargo.toml",
            "[package]\nname=\"x\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
        ),
        (
            "src/lib.rs",
            "pub trait Cmd{fn r(&self);}\npub struct A;\nimpl Cmd for A{fn r(&self){}}",
        ),
    ]
    .iter()
    .cloned()
    .collect();
    write_tree(root, &files);

    let (_fp, out) = run_characterize(root);
    let calibration = Calibration::default();
    let templates = Templates::new(None);
    emit(root, &out, &calibration, &templates, None, "author").expect("emit succeeds");

    let orient_text =
        std::fs::read_to_string(out.join("orientation.md")).expect("read orientation.md");
    let ref_text =
        std::fs::read_to_string(out.join("reference.md")).expect("read reference.md");
    assert!(orient_text.contains("[AGENT]"));
    assert!(ref_text.contains("src/lib.rs:"));
    assert!(orient_text.contains("trait_impl:Cmd"));
    assert!(
        orient_text.contains("Legend (kind -> pick group)"),
        "the histogram appendix carries the kind->group legend"
    );
}

#[test]
fn overlay_vis_backfill_admits_macro_invisible_pub_trait() {
    // The trait is declared through macro-invocation tokens WITHOUT a
    // pub token (the define_label!-class expansion hides visibility),
    // so the floor records blank vis and the public set rejects it
    // despite curated example evidence. A pass-local overlay whose
    // pub_traits carries the name flips is_pub at emit time and the
    // pick renders in 5.2.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let files: HashMap<&str, &str> = [
        ("Cargo.toml", "[workspace]\nmembers=[\"lib\",\"app\"]\n"),
        (
            "lib/Cargo.toml",
            "[package]\nname=\"lib\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
        ),
        (
            // Sched is declared NOWHERE the floor can see (the
            // define_label! class: a name-only invocation expands to
            // the pub trait) - the pm row carries defining_crate null
            // AND is_pub false; the overlay backfills both.
            "lib/src/lib.rs",
            "pub struct Core;\nimpl Core { pub fn new() -> Self { Core } }\n",
        ),
        (
            "lib/examples/demo.rs",
            "use lib::Sched;\n#[derive(Sched)]\nstruct D;\nfn main() {}\n",
        ),
        (
            "app/Cargo.toml",
            "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../lib\"}\n",
        ),
        (
            "app/src/lib.rs",
            "use lib::{Sched, Core};\n#[derive(Sched)]\npub struct A;\n#[derive(Sched)]\npub struct B;\npub fn touch() -> Core { Core::new() }\n",
        ),
    ]
    .iter()
    .cloned()
    .collect();
    write_tree(root, &files);

    let (_fp, out) = run_characterize(root);
    let calibration = Calibration::default();
    let templates = Templates::new(None);

    emit(root, &out, &calibration, &templates, None, "author").expect("plain emit");
    let plain = std::fs::read_to_string(out.join("orientation.md")).expect("read plain");
    let upper_tier = |text: &str| -> String {
        text.lines()
            .skip_while(|l| !l.starts_with("### 5.1"))
            .take_while(|l| !l.starts_with("### 5.3"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert!(
        !upper_tier(&plain).contains("`configuring:Sched`"),
        "blank-vis trait must not enter the public-qualified tiers without an overlay"
    );

    std::fs::write(
        out.join("rustdoc_overlay.json"),
        "{\"status\":\"ok\",\"package\":\"lib\",\"pub_traits\":[\"Sched\"],\"pub_types\":[]}",
    )
    .expect("write overlay");
    emit(root, &out, &calibration, &templates, None, "author").expect("overlay emit");
    let backfilled =
        std::fs::read_to_string(out.join("orientation.md")).expect("read backfilled");
    // The backfilled flag makes Sched public-qualified; with its
    // existing inter signal it promotes into ARCHITECTURE (public
    // AND inter), the top tier.
    assert!(
        upper_tier(&backfilled).contains("`configuring:Sched`"),
        "overlay pub_traits backfills is_pub; the pick renders in 5.1/5.2; got:\n{}",
        upper_tier(&backfilled)
    );
}
