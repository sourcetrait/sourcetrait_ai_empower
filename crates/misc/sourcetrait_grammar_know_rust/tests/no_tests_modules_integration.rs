//! Integration tests for the no-tests rule's out-of-line module
//! closure (R8 slice 1): files reached via `#[cfg(test)] mod x;`
//! declarations are excluded from both walkers, and the usages
//! scanner gains parity on inline cfg(test) items.

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
fn out_of_line_cfg_test_modules_excluded() {
    // lib declares `#[cfg(test)] mod tests;` (sibling form) and a
    // 2018-style nested `src/foo.rs` -> `src/foo/tests.rs`. Neither
    // test file's items may appear in the scan; production items do.
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
                "pub struct Real;\npub mod foo;\n#[cfg(test)]\nmod tests;\n",
            ),
        ),
        (
            "lib/src/tests.rs",
            String::from(
                "use crate::Real;\npub struct TestOnlyType;\npub fn check(_r: Real) -> TestOnlyType { TestOnlyType }\n",
            ),
        ),
        (
            "lib/src/foo.rs",
            String::from(
                "pub struct FooReal;\n#[cfg(test)]\n#[allow(dead_code)]\npub mod tests;\n",
            ),
        ),
        (
            "lib/src/foo/tests.rs",
            String::from("pub struct NestedTestType;\n"),
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
                "pub fn take(_r: lib::Real, _f: lib::foo::FooReal) {}\n",
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
    let facts: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.join("facts.json")).expect("read facts"),
    )
    .expect("parse facts");

    let type_names: Vec<&str> = facts
        .get("types")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
                .collect()
        })
        .unwrap_or_default();
    assert!(type_names.contains(&"Real"), "production type scanned");
    assert!(type_names.contains(&"FooReal"), "nested production type scanned");
    assert!(
        !type_names.contains(&"TestOnlyType"),
        "sibling-form test module excluded; got {:?}",
        type_names
    );
    assert!(
        !type_names.contains(&"NestedTestType"),
        "2018-style nested test module excluded; got {:?}",
        type_names
    );
    let pm = fp
        .get("pattern_metrics")
        .and_then(|v| v.as_object())
        .expect("pattern_metrics");
    assert!(
        !pm.keys().any(|k| k.contains("TestOnlyType") || k.contains("NestedTestType")),
        "no test-module pattern keys; got {:?}",
        pm.keys().filter(|k| k.contains("Test")).collect::<Vec<_>>()
    );
}

#[test]
fn inline_cfg_test_mod_feeds_no_usage_signals() {
    // The usages scanner must skip inline #[cfg(test)] items entirely
    // (parity with the items walker). InlineSeen appears ONLY inside
    // the gated mod's fn signature.
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    let files: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from(
                "[package]\nname=\"solo\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        (
            "src/lib.rs",
            String::from(
                "pub struct InlineSeen;\n\
                 #[cfg(test)]\nmod itest {\n    pub fn helper(_x: crate::InlineSeen) {}\n}\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(root, &files);

    let out = root.join(".usages");
    std::fs::create_dir_all(&out).expect("mkdir out");
    scan_usages(root, &out).expect("scan_usages succeeds");
    let usages: UsageFacts = serde_json::from_str(
        &std::fs::read_to_string(out.join("know_rust_usages.json")).expect("read usages"),
    )
    .expect("parse UsageFacts");
    assert!(
        !usages
            .ast_fn_sig_usages
            .iter()
            .any(|u| u.container.contains("itest")),
        "no fn-sig usages from inside the inline cfg(test) mod"
    );
    assert!(
        !usages
            .ast_fn_sig_usages
            .iter()
            .any(|u| u.ident == "InlineSeen"),
        "InlineSeen reached only via the gated mod; must be absent"
    );
}
