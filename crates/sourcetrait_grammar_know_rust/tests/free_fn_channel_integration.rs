//! Integration tests for the utilities FreeFn capture channel:
//! call heads of bare / crate-qualified pub fns synthesize
//! `utilities:<fn>` picks; turbofish type arguments are real type
//! usage. Surfaced by the nu_sh_mcp consumer trace: nushell's
//! embedding API (eval_block / parse / create_default_context) was
//! invisible to every pick channel.

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
fn free_fn_calls_feed_utilities_and_turbofish_credits_types() {
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
                "pub fn helper_thing() {}\n\
                 pub struct Marker;\nimpl Default for Marker { fn default() -> Self { Marker } }\n\
                 pub fn typed_default<T: Default>() -> T { T::default() }\n\
                 pub mod util { pub fn modfn() {} }\n",
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
                "use lib::{helper_thing, typed_default, util, Marker};\n\
                 fn local_one() {}\n\
                 pub fn run() {\n\
                     helper_thing();\n\
                     let _m: Marker = typed_default::<Marker>();\n\
                     util::modfn();\n\
                     drop(3);\n\
                     local_one();\n\
                 }\n",
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

    let helper = pm
        .get("utilities:helper_thing")
        .expect("bare imported pub-fn call synthesizes a utilities pick");
    assert_eq!(
        helper.get("defining_crate").and_then(|v| v.as_str()),
        Some("lib"),
        "attribution follows the import"
    );
    assert_eq!(
        helper.get("sub_form").and_then(|v| v.as_str()),
        Some("free_fn"),
        "non-macro utilities classify FreeFn"
    );
    assert!(
        pm.contains_key("utilities:typed_default"),
        "turbofish call head still captured as a free fn"
    );
    assert!(
        pm.contains_key("structure:Marker"),
        "turbofish type argument credits the type; structure keys: {:?}",
        pm.keys().filter(|k| k.starts_with("structure:")).collect::<Vec<_>>()
    );
    assert!(
        !pm.contains_key("utilities:modfn"),
        "mod-qualified calls stay with the implementation_functions channel"
    );
    assert!(
        pm.contains_key("implementation_functions:util::modfn"),
        "the mod-fn channel keeps mod-qualified calls"
    );
    assert!(
        !pm.contains_key("utilities:drop"),
        "std prelude fns never become picks"
    );
    assert!(
        !pm.contains_key("utilities:local_one"),
        "private fns never become picks"
    );
}
