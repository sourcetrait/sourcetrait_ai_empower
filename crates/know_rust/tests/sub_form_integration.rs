//! Integration tests for R4a form sub-classifiers + prose-budget matrix.
//!
//! Synthetic Rust workspaces drive the classifier through characterize +
//! emit; the resulting fingerprint.json + orientation.md are inspected
//! to confirm sub_form classification + budget_hint emission. Matrix
//! lookup unit tests live in src/config/calibration.rs's #[cfg(test)]
//! module; this file covers the classifier heuristics + the end-to-end
//! emit-side wiring.

use know_rust::*;
use std::collections::HashMap;
use std::path::Path;
use tempfile::TempDir;

/// What: write `files` into `base`. Each entry's key is a workspace-
/// relative path; parents are created lazily.
fn write_tree(base: &Path, files: &HashMap<&str, String>) {
    for (rel, content) in files {
        let p = base.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("mkdir parent");
        }
        std::fs::write(&p, content).expect("write file");
    }
}

/// What: run characterize against `root` and return the parsed
/// fingerprint.json + the out directory containing the rest of the
/// produced artifacts.
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

/// What: read `pattern_metrics.<key>.sub_form` from a parsed
/// fingerprint and return the wire token.
fn sub_form_for(fp: &serde_json::Value, pattern_key: &str) -> Option<String> {
    fp.get("pattern_metrics")
        .and_then(|v| v.as_object())
        .and_then(|m| m.get(pattern_key))
        .and_then(|m| m.get("sub_form"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

#[test]
fn classify_traits_lifecycle_at_five_impl_threshold() {
    // The classifier marks a trait Lifecycle when it has >= 5
    // non-cfg-gated workspace impls. We construct a workspace where
    // trait `Behavior` has exactly 5 impls and trait `Bare` has 4,
    // and assert the classifier splits them across the threshold.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let mut game_lib = String::from("use ecs::{Behavior, Bare};\n");
    for i in 0..5 {
        game_lib.push_str(&format!(
            "pub struct A{i};\nimpl Behavior for A{i} {{ fn run(&self) {{}} }}\n"
        ));
    }
    for i in 0..4 {
        game_lib.push_str(&format!("pub struct B{i};\nimpl Bare for B{i} {{}}\n"));
    }
    let files: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from("[workspace]\nmembers=[\"ecs\",\"game\"]\n"),
        ),
        (
            "ecs/Cargo.toml",
            String::from(
                "[package]\nname=\"ecs\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        (
            "ecs/src/lib.rs",
            String::from("pub trait Behavior { fn run(&self); }\npub trait Bare {}\n"),
        ),
        (
            "game/Cargo.toml",
            String::from(
                "[package]\nname=\"game\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\necs={path=\"../ecs\"}\n",
            ),
        ),
        ("game/src/lib.rs", game_lib),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let (fp, _out) = run_characterize(root);
    assert_eq!(
        sub_form_for(&fp, "traits:Behavior").as_deref(),
        Some("lifecycle"),
        "trait with 5 impls should classify lifecycle"
    );
    assert_eq!(
        sub_form_for(&fp, "traits:Bare").as_deref(),
        Some("marker"),
        "trait with 4 impls should classify marker"
    );
}

#[test]
fn configuring_group_has_no_mechanical_sub_form() {
    // A workspace-defined derive lands in the `configuring` group (the
    // broadened/renamed Derives group) with NO mechanical sub_form: the
    // mechanical layer marks the broad group, the subagent thought-
    // experiment does the fine subclassification (mechanical-broad,
    // subagent-fine; the per-subject `configured_derives` allowlist
    // cheat was removed in the Configuring rename). The former
    // `derives:Component` key is now `configuring:Component`.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let files: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from("[workspace]\nmembers=[\"ecs\",\"game\"]\n"),
        ),
        (
            "ecs/Cargo.toml",
            String::from(
                "[package]\nname=\"ecs\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        (
            "ecs/src/lib.rs",
            String::from("pub trait Component {}\n"),
        ),
        (
            "game/Cargo.toml",
            String::from(
                "[package]\nname=\"game\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\necs={path=\"../ecs\"}\n",
            ),
        ),
        (
            "game/src/lib.rs",
            String::from("use ecs::Component;\n#[derive(Component)]\npub struct Pos;\n"),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let (fp, _out) = run_characterize(root);
    let pm = fp
        .get("pattern_metrics")
        .and_then(|v| v.as_object())
        .expect("pattern_metrics object");
    assert!(
        pm.contains_key("configuring:Component"),
        "derive lands under configuring:Component; keys with Component: {:?}",
        pm.keys()
            .filter(|k| k.contains("Component"))
            .collect::<Vec<_>>()
    );
    assert!(
        !pm.contains_key("derives:Component"),
        "the renamed group must not emit the old derives: key"
    );
    // No mechanical sub_form for the configuring group (subagent-fine).
    assert_eq!(
        sub_form_for(&fp, "configuring:Component"),
        None,
        "configuring carries no mechanical sub_form"
    );
}

#[test]
fn classify_structure_foundational_threshold_30() {
    // The classifier marks a structure Foundational when intra +
    // inter + example >= 30. We construct a workspace where struct
    // `Core` is used 30 times (above threshold) and struct `Tiny`
    // is used once.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let mut app_lib = String::from("use lib::{Core, Tiny};\n");
    for i in 0..30 {
        app_lib.push_str(&format!(
            "pub fn use_core_{i}() {{ let _x = Core::new(); }}\n"
        ));
    }
    app_lib.push_str("pub fn use_tiny() { let _t = Tiny::new(); }\n");
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
                "pub struct Core; impl Core { pub fn new() -> Self { Core } }\npub struct Tiny; impl Tiny { pub fn new() -> Self { Tiny } }\n",
            ),
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

    let (fp, _out) = run_characterize(root);
    assert_eq!(
        sub_form_for(&fp, "structure:Core").as_deref(),
        Some("foundational"),
        "Core with 30 usages should classify foundational"
    );
    assert_eq!(
        sub_form_for(&fp, "structure:Tiny").as_deref(),
        Some("incidental"),
        "Tiny with 1 usage should classify incidental"
    );
}

#[test]
fn classify_utilities_macro_for_macro_sourced_patterns() {
    // The picker emits utilities only from macro facts (reg_macro /
    // attr_macro). Each pattern's sub_form should classify Macro
    // because the matching MacroEntry surfaces the name.
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
                "#[macro_export]\nmacro_rules! bind_command { ($($t:ty),*) => {}; }\n",
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
                "pub fn boot() { bind_command!(); bind_command!(); bind_command!(); }\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let (fp, _out) = run_characterize(root);
    assert_eq!(
        sub_form_for(&fp, "utilities:bind_command").as_deref(),
        Some("macro"),
        "macro-sourced utilities classify Macro"
    );
}

#[test]
fn orientation_emits_budget_suffix_on_s5_picks() {
    // End-to-end smoke: a small workspace surfaces at least one S5
    // pick whose orientation bullet carries the ` - budget N` suffix
    // emitted by R4a. The exact set the pick lands in varies with
    // the synthetic shape; the assertion is the suffix-presence
    // contract.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let mut app_lib = String::from("use lib::{Plug, Comp};\n");
    for i in 0..7 {
        app_lib.push_str(&format!(
            "pub struct P{i};\nimpl Plug for P{i} {{ fn build(&self) {{}} }}\n"
        ));
    }
    for i in 0..30 {
        app_lib.push_str(&format!(
            "pub fn use_comp_{i}() {{ let _c = Comp::default(); }}\n"
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
            String::from(
                "pub trait Plug { fn build(&self); }\npub struct Comp; impl Comp { pub fn default() -> Self { Comp } }\n",
            ),
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

    let (_fp, out) = run_characterize(root);
    let calibration = Calibration::default();
    let templates = Templates::new(None);
    emit(root, &out, &calibration, &templates, None).expect("emit succeeds");
    let orient_text =
        std::fs::read_to_string(out.join("orientation.md")).expect("read orientation.md");

    // Every S5 pick line should carry the budget suffix.
    let pick_lines: Vec<&str> = orient_text
        .lines()
        .filter(|l| {
            l.starts_with("- `")
                && (l.contains(" - architecture score ")
                    || l.contains(" - public score ")
                    || l.contains(" - inter_count ")
                    || l.contains(" - clique votes ")
                    || l.contains(" occurrences "))
        })
        .collect();
    assert!(
        !pick_lines.is_empty(),
        "expected at least one S5 pick bullet, got 0; orientation excerpt:\n{}",
        orient_text.lines().take(60).collect::<Vec<_>>().join("\n")
    );
    for line in &pick_lines {
        assert!(
            line.contains(" - budget "),
            "S5 pick bullet missing ' - budget ' suffix: {}",
            line
        );
    }
}

#[test]
fn orientation_budget_suffix_carries_sub_form_when_classified() {
    // When a pick's sub_form is classified, the orientation suffix
    // should be ` - budget N (sub_form_wire)`. We construct a
    // workspace where structure Comp lands as foundational and
    // assert the orientation bullet for it has the foundational
    // annotation.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let mut app_lib = String::from("use lib::Comp;\n");
    for i in 0..30 {
        app_lib.push_str(&format!(
            "pub fn use_comp_{i}() {{ let _c = Comp::default(); }}\n"
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
            String::from(
                "pub struct Comp; impl Comp { pub fn default() -> Self { Comp } }\n",
            ),
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

    let (_fp, out) = run_characterize(root);
    let calibration = Calibration::default();
    let templates = Templates::new(None);
    emit(root, &out, &calibration, &templates, None).expect("emit succeeds");
    let orient_text =
        std::fs::read_to_string(out.join("orientation.md")).expect("read orientation.md");

    let comp_line = orient_text
        .lines()
        .find(|l| l.starts_with("- `structure:Comp`"));
    let comp_line = comp_line.expect("structure:Comp bullet expected in S5");
    assert!(
        comp_line.contains(" - budget ") && comp_line.contains("(foundational)"),
        "structure:Comp should carry foundational suffix: {}",
        comp_line
    );
}

#[test]
fn nf5_test_helper_substring_filter_drops_type_usages() {
    // NF5 noise filter: a workspace whose lib defines Value with both
    // a test_string helper and a production int factory, and an app
    // that calls both, should yield pattern_metrics with the
    // production pattern but NOT the test_-prefixed one. The default
    // `["::test_"]` pattern_skip_substrings catches the noise family
    // at compute_pattern_metrics ingestion; the aggregated
    // structure:Value carry only sees the production signal.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let lib_src = String::from(
        "pub struct Value;\n\
         impl Value {\n\
             pub fn test_string() -> Self { Value }\n\
             pub fn int() -> Self { Value }\n\
         }\n\
         pub mod util { pub fn _var_for_test_state() -> u8 { 0 } pub fn var_state() -> u8 { 0 } }\n",
    );
    let app_src = String::from(
        "use lib::{Value, util};\n\
         pub fn run() -> (Value, Value) {\n\
             let _a = util::_var_for_test_state();\n\
             let _b = util::var_state();\n\
             (Value::test_string(), Value::int())\n\
         }\n",
    );
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
        (
            "app/Cargo.toml",
            String::from(
                "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../lib\"}\n",
            ),
        ),
        ("app/src/lib.rs", app_src),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let (fp, _out) = run_characterize(root);
    let pm = fp
        .get("pattern_metrics")
        .and_then(|v| v.as_object())
        .expect("pattern_metrics object");

    assert!(
        !pm.contains_key("implementation_functions:Value::test_string"),
        "implementation_functions:Value::test_string should be dropped by NF5; \
         found keys with 'test_': {:?}",
        pm.keys()
            .filter(|k| k.contains("test_"))
            .collect::<Vec<_>>(),
    );
    assert!(
        pm.contains_key("implementation_functions:Value::int"),
        "implementation_functions:Value::int should survive NF5"
    );
    assert!(
        pm.contains_key("structure:Value"),
        "structure:Value should still aggregate from Value::int"
    );
    // The infix `_test_` substring catches the _var_for_test_* shape
    // in BOTH channels: the usage stream and the decl-channel mint.
    assert!(
        !pm.keys().any(|k| k.contains("_var_for_test_state")),
        "infix _test_ shape dropped (usage + decl mint); keys: {:?}",
        pm.keys().filter(|k| k.contains("test")).collect::<Vec<_>>()
    );
    assert!(
        pm.contains_key("implementation_functions:util::var_state"),
        "sibling production fn unaffected by the _test_ substring"
    );
}

#[test]
fn carry_gated_to_workspace_origin() {
    // Picks (all forms, including Carried) are workspace-origin only. The
    // walker records carry structurally; characterize gates it to
    // workspace-declared types/traits. An impl on a std target (Vec) must
    // not yield a structure:Vec carry key, and a workspace struct's carry
    // must drop std names (Vec, String) while keeping workspace names
    // (Inner, Marker). See notes/know_rust/working/02_picks_data.md.
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
                "pub trait Marker {}\npub struct Inner;\npub struct Outer<T: Marker> { pub inner: Inner, pub items: Vec<String> }\nimpl<T: Clone> Marker for Vec<T> {}\n",
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
                "use lib::{Marker, Inner, Outer};\npub fn use_them() { let _i = Inner; let _o: Option<Outer<u8>> = None; }\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let (_fp, out) = run_characterize(root);
    let facts: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.join("facts.json")).expect("read facts.json"),
    )
    .expect("parse facts.json");
    let carries = facts
        .get("carries")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    // external impl target -> no carry key
    assert!(
        !carries.contains_key("structure:Vec"),
        "structure:Vec (std impl target) must be gated out; keys: {:?}",
        carries.keys().collect::<Vec<_>>()
    );

    // workspace struct keeps workspace carry, drops std names
    let outer_names: Vec<String> = carries
        .get("structure:Outer")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|e| e.get("name").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        outer_names.contains(&"Inner".to_string()),
        "Outer keeps workspace carry Inner; got {:?}",
        outer_names
    );
    assert!(
        outer_names.contains(&"Marker".to_string()),
        "Outer keeps workspace bound Marker; got {:?}",
        outer_names
    );
    assert!(
        !outer_names.contains(&"Vec".to_string()),
        "Outer drops std carry Vec; got {:?}",
        outer_names
    );
    assert!(
        !outer_names.contains(&"String".to_string()),
        "Outer drops std carry String; got {:?}",
        outer_names
    );
}
