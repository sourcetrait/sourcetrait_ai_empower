use std::path::Path;

use serde_json::{Value, json};
use sourcetrait_grammar_mcp::guts::{TestServer, has_error, valid_function_source, write_source};
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

/// Read a committed golden JSON under `<crate>/testing/goldens/`, comparing it to
/// `actual`. `BLESS=1 cargo test` (re)writes it from the actual value; without the
/// golden present (and no BLESS) the read fails loudly. Goldens capture stable
/// serialize-shaped output so a shape change is a reviewable file diff.
fn golden(name: &str, actual: &Value) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("testing/goldens").join(name);
    if std::env::var("BLESS").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir goldens");
        std::fs::write(&path, serde_json::to_string_pretty(actual).expect("serialize golden"))
            .expect("write golden");
    }
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read golden {name}: {e} (run with BLESS=1 to (re)generate)"));
    serde_json::from_str(&text).expect("parse golden")
}

fn find_node<'a>(nodes: &'a Value, name: &str) -> &'a Value {
    nodes
        .as_array()
        .expect("node array")
        .iter()
        .find(|n| n["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("node `{name}` not found in {nodes}"))
}

#[test]
fn info_returns_static_server_state() {
    let s = TestServer::new();
    let env = s.info();
    assert_eq!(env["name"].as_str(), Some("grammar"), "name should be grammar; got {env}");

    let version = env["version"].as_str().expect("version field");
    assert_eq!(
        version,
        env!("CARGO_PKG_VERSION"),
        "version should match crate CARGO_PKG_VERSION",
    );

    let nu_version = env["nu_version"].as_str().expect("nu_version field");
    assert!(
        !nu_version.is_empty() && nu_version.contains('.'),
        "nu_version should be non-empty + semver-shaped; got {nu_version:?}",
    );

    let plugins = env["plugins"].as_array().expect("plugins should be an array");
    for p in plugins {
        let entry = p
            .as_array()
            .expect("each plugin is a positional [name, version] pair");
        assert_eq!(entry.len(), 2, "plugin entry is a 2-element [name, version]; got {entry:?}");
        let name = entry[0].as_str().expect("plugin name is a string");
        assert!(!name.is_empty(), "plugin name should be non-empty");
        assert!(
            entry[1].is_string() || entry[1].is_null(),
            "plugin version slot should be string-or-null; got {:?}",
            entry[1],
        );
    }
    // The "fresh store -> 0 libraries" property belongs to the SYSTEM tier (a per-store
    // fact: id_namespace::namespaces_are_disjoint_stores checks a fresh namespace is empty).
    // This binary's in-process store is shared across its tests, so no empty-libraries
    // assertion is made here.
}

#[test]
#[named]
fn info_lists_committed_library_hierarchy() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("treelib");
    let est = s.library("new", "sourcetrait/treelib", src.to_str().unwrap());
    assert!(!has_error(&est), "establish failed: {est}");
    for (module_path, name, args_inner, result_inner, body) in [
        ("solo", "rootfn", "x: int", "out: int", "{ out: ($args.x + 1) }"),
        (
            "alpha",
            "a1",
            "s: string",
            "len: int",
            "{ len: ($args.s | str length) }",
        ),
        (
            "alpha/beta",
            "b1",
            "x: int, t: record<y: string, n: list<int>>",
            "out: record<y: string>",
            "{ out: { y: $args.t.y } }",
        ),
    ] {
        let np = format!("sourcetrait/treelib:{module_path}:{name}");
        let scaffold = s.scaffold(&[np.as_str()]);
        assert!(!has_error(&scaffold), "scaffold {np} failed: {scaffold}");
        let rel = format!("{module_path}/{name}/mod.nu");
        write_source(&src, &rel, &valid_function_source(args_inner, result_inner, body));
    }
    let committed = s.commit("sourcetrait/treelib");
    assert!(!has_error(&committed), "commit failed: {committed}");

    let info = s.info();
    let libs = info["libraries"].as_array().expect("libraries array");
    let mut lib = libs
        .iter()
        .find(|l| l["name"].as_str() == Some("sourcetrait/treelib"))
        .expect("treelib present in info()")
        .clone();
    // `path` is the volatile per-run temp source dir: assert it inline, then drop it
    // from the golden comparison (the golden captures the stable hierarchy shape).
    assert_eq!(lib["path"].as_str(), src.to_str(), "path should be the meta source_path");
    lib.as_object_mut().unwrap().remove("path");

    let expected = golden("info_treelib.json", &lib);
    assert_eq!(lib, expected, "info() treelib hierarchy drifted from the golden");
}

#[test]
#[named]
fn info_lists_hand_authored_library_hierarchy() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("implib");
    let est = s.library("new", "sourcetrait/implib", src.to_str().unwrap());
    assert!(!has_error(&est), "establish failed: {est}");
    write_source(&src, "mod.nu", "export module math\n");
    write_source(&src, "math/mod.nu", "export use double\n");
    write_source(
        &src,
        "math/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );

    let committed = s.commit("sourcetrait/implib");
    assert!(!has_error(&committed), "commit failed: {committed}");

    let info = s.info();
    let libs = info["libraries"].as_array().expect("libraries array");
    let lib = libs
        .iter()
        .find(|l| l["name"].as_str() == Some("sourcetrait/implib"))
        .expect("implib present in info()");
    assert_eq!(lib["path"].as_str(), src.to_str(), "path should be the source directory");
    assert!(lib["functions"].as_array().expect("root fns").is_empty());
    let modules = lib["modules"].as_array().expect("modules");
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0]["name"].as_str(), Some("math"));
    assert!(
        modules[0]["submodules"]
            .as_array()
            .expect("math submodules")
            .is_empty(),
    );
    let math_fns = modules[0]["functions"].as_array().expect("math fns");
    assert_eq!(math_fns.len(), 1);
    assert_eq!(math_fns[0]["name"].as_str(), Some("double"));
    assert_eq!(math_fns[0]["args_schema"], json!({"x": "int"}));
    assert_eq!(math_fns[0]["result_schema"], json!({"out": "int"}));
}

#[test]
#[named]
fn info_includes_node_summaries() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("doctreelib");
    let _ = s.library("new", "sourcetrait/doctreelib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "# the doctree library\nexport module m\n");
    write_source(&src, "m/mod.nu", "# the m module\nexport use fn\n");
    write_source(
        &src,
        "m/fn/mod.nu",
        "# the fn summary\nexport def main [args: record<x: int>]: nothing -> record<out: int> { { out: $args.x } }\n",
    );
    let committed = s.commit("sourcetrait/doctreelib");
    assert!(!has_error(&committed), "commit failed: {committed}");

    let info = s.info();
    let libs = info["libraries"].as_array().expect("libraries array");
    let lib = libs
        .iter()
        .find(|l| l["name"].as_str() == Some("sourcetrait/doctreelib"))
        .expect("doctreelib present in info()");
    assert_eq!(lib["summary"].as_str(), Some("the doctree library"));
    let module = find_node(&lib["modules"], "m");
    assert_eq!(module["summary"].as_str(), Some("the m module"));
    let fns = module["functions"].as_array().expect("functions");
    assert_eq!(fns[0]["name"].as_str(), Some("fn"));
    assert_eq!(fns[0]["summary"].as_str(), Some("the fn summary"));
}
