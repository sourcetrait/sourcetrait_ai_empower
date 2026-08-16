//! Integration tests for `know_rust measure demand` (phase-1
//! conversion of the kr_consumer_trace.py prototype): demand from a
//! consumer workspace, coverage against the target's picks + carry,
//! rename translation (same-file binding and crate-wide alias),
//! pair tiers, and the zero-miss exit gate.

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

fn build_target(root: &Path) -> std::path::PathBuf {
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
                "pub struct Value;\nimpl Value { pub fn new() -> Self { Value } }\n\
                 pub struct Hidden;\n\
                 pub fn boot() {}\n",
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
                "use lib::{Value, boot};\n\
                 pub fn a() { let _ = Value::new(); }\n\
                 pub fn b() { let _ = Value::new(); }\n\
                 pub fn c() { boot(); }\n",
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
    out
}

fn consumer_files(with_hidden: bool) -> HashMap<&'static str, String> {
    let lib_rs = if with_hidden {
        "pub(crate) mod inner;\n\
         use lib::Value as JsonVal;\n\
         use lib::{Hidden, boot};\n\
         pub fn run(_h: Hidden) { boot(); let _ = JsonVal::new(); }\n"
    } else {
        // The full-path glob (`use lib::*;`, no brace group) must
        // land in the GLOBS bucket, not register a demanded name
        // `*` (the suffix-glob parse bug found by the bevy_ahoy
        // consumer trace).
        "pub(crate) mod inner;\n\
         use lib::Value as JsonVal;\n\
         use lib::boot;\n\
         use lib::*;\n\
         pub fn run() { boot(); let _ = JsonVal::new(); }\n"
    };
    [
        (
            "Cargo.toml",
            String::from(
                "[package]\nname=\"consumer\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        ("src/lib.rs", lib_rs.to_string()),
        (
            // Cross-file alias: JsonVal is bound in lib.rs; this file
            // reaches it through an internal module path, so only the
            // crate-wide rename map can translate it back to Value.
            "src/inner.rs",
            String::from("pub fn go() { let _ = api::JsonVal::make(); }\npub mod api {}\n"),
        ),
    ]
    .into_iter()
    .collect()
}

#[test]
fn demand_resolves_lib_and_dep_renames() {
    // Target package `tgt-kit` exposes lib name `tkit`; the consumer
    // imports through the LIB binding in one file and through its own
    // DEP RENAME (`tk = { package = "tgt-kit" }`) in another. Both
    // demands must resolve and be covered - the package-name-only
    // vocabulary registered nothing for either form.
    let target_tmp = TempDir::new().expect("target tempdir");
    let troot = target_tmp.path();
    let tfiles: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from("[workspace]\nmembers=[\"tgt-kit\",\"app\"]\n"),
        ),
        (
            "tgt-kit/Cargo.toml",
            String::from(
                "[package]\nname=\"tgt-kit\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[lib]\nname=\"tkit\"\n[dependencies]\n",
            ),
        ),
        (
            "tgt-kit/src/lib.rs",
            String::from(
                "pub struct Value;\nimpl Value { pub fn new() -> Self { Value } }\npub fn boot() {}\n",
            ),
        ),
        (
            "app/Cargo.toml",
            String::from(
                "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\ntgt-kit={path=\"../tgt-kit\"}\n",
            ),
        ),
        (
            "app/src/lib.rs",
            String::from(
                "use tkit::{Value, boot};\npub fn a() { let _ = Value::new(); }\npub fn b() { let _ = Value::new(); }\npub fn c() { boot(); }\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(troot, &tfiles);
    let tout = troot.join(".orientation");
    std::fs::create_dir_all(&tout).expect("mkdir orientation");
    let calibration = Calibration::default();
    characterize(troot, &tout, &calibration).expect("characterize succeeds");
    let templates = Templates::new(None);
    emit(troot, &tout, &calibration, &templates, None, "author").expect("emit succeeds");

    let ctmp = TempDir::new().expect("consumer tempdir");
    let cfiles: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from(
                "[package]\nname=\"consumer\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\ntk={package=\"tgt-kit\", path=\"../t/tgt-kit\"}\n",
            ),
        ),
        (
            "src/lib.rs",
            String::from(
                "pub(crate) mod inner;\nuse tkit::Value;\npub fn a() { let _ = Value::new(); }\n",
            ),
        ),
        (
            "src/inner.rs",
            String::from("use tk::boot;\npub fn b() { boot(); }\n"),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(ctmp.path(), &cfiles);

    let out_path = ctmp.path().join("trace_out.json");
    measure_demand(ctmp.path(), &tout, Some(&out_path))
        .expect("lib-binding + dep-rename demands are covered");
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&out_path).expect("read --out json"),
    )
    .expect("parse --out json");
    let hit_names: Vec<&str> = report
        .get("hits")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.get("name").and_then(|v| v.as_str()))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        hit_names.contains(&"Value"),
        "lib-binding demand resolves; hits: {hit_names:?}"
    );
    assert!(
        hit_names.contains(&"boot"),
        "dep-rename demand resolves; hits: {hit_names:?}"
    );
}

#[test]
fn crate_qualified_call_is_served_by_the_pair_pick() {
    // The tokio::spawn shape: the target declares the fn inside a
    // macro template (so the free-fn channel sees no pub visibility
    // and synthesizes NO utilities pick), while target-internal
    // crate-qualified calls render the pick as the PAIR key
    // `implementation_functions:lib::helper`. A consumer calling
    // `lib::helper()` demands the bare name `helper` through root
    // `lib` - the end-product criterion says the pair pick serves
    // it (the reading agent finds lib::helper in the bundle), so
    // the trace must score a hit, not a miss.
    let tmp = TempDir::new().expect("target tempdir");
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
                "macro_rules! decl_api { () => { pub fn helper() {} }; }\ndecl_api!();\npub mod m { pub fn mfn() {} pub fn mfn2() {} }\npub struct Core;\nimpl Core { pub fn new() -> Self { Core } }\npub fn seed() -> Core { Core::new() }\npub fn seed2() -> Core { Core::new() }\n",
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
                "pub fn run(_c: lib::Core) { lib::helper(); lib::m::mfn(); lib::m::mfn2(); }\npub fn run2() { lib::helper(); lib::m::mfn(); lib::m::mfn2(); }\n",
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
    let orient = std::fs::read_to_string(out.join("orientation.md")).expect("read orientation");
    assert!(
        orient.contains("`implementation_functions:lib::helper`"),
        "fixture premise: the pair pick renders"
    );
    assert!(
        !orient.contains("`utilities:helper`"),
        "fixture premise: no bare utilities pick covers the name"
    );
    assert!(
        orient.contains("`implementation_functions:m::mfn`"),
        "fixture premise: the module-fn pair pick renders"
    );
    assert!(
        orient.contains("`implementation_functions:m::mfn2`"),
        "fixture premise: the second module-fn pair pick renders"
    );

    let ctmp = TempDir::new().expect("consumer tempdir");
    let cfiles: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from(
                "[package]\nname=\"consumer\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../t/lib\"}\n",
            ),
        ),
        (
            // helper: crate-qualified call -> served by the
            // `lib::helper` pair. mfn: module-pathed import (nested
            // brace piece) + bare call -> the leaf's PARENT segment
            // serves it through the `m::mfn` pair. mfn2: full-path
            // call with NO import -> only the CALL-SITE parent
            // segment can reach the `m::mfn2` pair (the 0.0.37
            // TRACE-BLIND shape: cosmic::iced::stream::channel).
            "src/lib.rs",
            String::from(
                "use lib::{m::mfn, Core};\npub fn go(_c: Core) { lib::helper(); mfn(); }\npub fn go2() { lib::m::mfn2(); }\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(ctmp.path(), &cfiles);

    let out_path = ctmp.path().join("trace_out.json");
    measure_demand(ctmp.path(), &out, Some(&out_path))
        .expect("crate-qualified + module-pathed demands are served by pair picks");
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&out_path).expect("read --out json"),
    )
    .expect("parse --out json");
    let hit_names: Vec<&str> = report
        .get("hits")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.get("name").and_then(|v| v.as_str()))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        hit_names.contains(&"helper"),
        "helper served via the lib::helper pair pick; hits: {hit_names:?}"
    );
    assert!(
        hit_names.contains(&"mfn"),
        "mfn served via the m::mfn pair pick (use-path penultimate); hits: {hit_names:?}"
    );
    assert!(
        hit_names.contains(&"mfn2"),
        "mfn2 served via the m::mfn2 pair pick (call-site parent); hits: {hit_names:?}"
    );
    assert_eq!(
        report.pointer("/summary/miss_count").and_then(|v| v.as_u64()),
        Some(0),
        "no name misses"
    );
}

#[test]
fn miss_gates_and_clean_run_passes() {
    let target_tmp = TempDir::new().expect("target tempdir");
    let target_out = build_target(target_tmp.path());

    // v1: demands Hidden (declared in the target but never picked or
    // carried) -> one name miss, nonzero gate.
    let c1 = TempDir::new().expect("consumer tempdir");
    write_tree(c1.path(), &consumer_files(true));
    let err = measure_demand(c1.path(), &target_out, None)
        .expect_err("uncovered demand must gate");
    assert_eq!(
        err.to_string(),
        "demand misses: 1 name(s), 0 pair(s) uncovered",
        "expected 1 name miss, 0 pair misses"
    );
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(target_out.join("consumer_trace.json"))
            .expect("trace json written even on miss"),
    )
    .expect("parse trace json");
    let miss_names: Vec<&str> = report
        .pointer("/summary/misses")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.get("name").and_then(|v| v.as_str()))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(miss_names, vec!["Hidden"], "the miss is Hidden");

    // v2: Hidden dropped -> clean run, exit Ok, rename-translated
    // demand and pair tiers as designed.
    let c2 = TempDir::new().expect("consumer2 tempdir");
    write_tree(c2.path(), &consumer_files(false));
    let out_path = c2.path().join("trace_out.json");
    measure_demand(c2.path(), &target_out, Some(&out_path))
        .expect("clean run passes the gate");
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&out_path).expect("read --out json"),
    )
    .expect("parse --out json");
    assert_eq!(
        report.pointer("/summary/miss_count").and_then(|v| v.as_u64()),
        Some(0)
    );
    assert_eq!(
        report.pointer("/summary/pair_miss_count").and_then(|v| v.as_u64()),
        Some(0)
    );
    let globs: Vec<&str> = report
        .pointer("/summary/globs")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(
        globs,
        vec!["lib::*"],
        "full-path glob lands in the globs bucket, not as a `*` name miss"
    );
    let hit_names: Vec<&str> = report
        .get("hits")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.get("name").and_then(|v| v.as_str()))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        hit_names.contains(&"Value"),
        "JsonVal demand rename-translates to Value; hits: {hit_names:?}"
    );
    assert!(
        hit_names.contains(&"boot"),
        "bare fn-call demand covered by the utilities pick; hits: {hit_names:?}"
    );
    assert!(
        !hit_names.contains(&"JsonVal"),
        "the alias name itself is not demand"
    );
    // Pair tiers: JsonVal::new -> Value::new is an exact pick;
    // api::JsonVal::make (cross-file alias) -> Value::make is
    // name-level (Value covered, no such pick).
    assert_eq!(
        report.pointer("/summary/pair_exact").and_then(|v| v.as_u64()),
        Some(1),
        "Value::new is the exact pair"
    );
    let name_level: Vec<&str> = report
        .get("pair_name_level")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    assert!(
        name_level.contains(&"Value::make"),
        "cross-file alias pair lands name-level; got {name_level:?}"
    );
}

#[test]
fn foreign_reexport_demands_bucket_without_gating() {
    // The target re-exports a FOREIGN crate's item (pub use
    // extfut::SinkExt) and namespace (pub use extfut;). Consumer
    // demands through both spellings are real demand the workspace
    // can never serve - they classify into the non-gating
    // foreign_reexport bucket and the run passes the zero-miss gate.
    let tmp = TempDir::new().expect("target tempdir");
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
                "pub use extfut::SinkExt;\npub use extfut;\npub struct Core;\nimpl Core { pub fn new() -> Self { Core } }\npub fn boot() {}\n",
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
                "use lib::{Core, boot};\npub fn a() { let _ = Core::new(); boot(); }\npub fn b() { let _ = Core::new(); }\n",
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

    // The serving half: orientation renders the foreign surface.
    let orient =
        std::fs::read_to_string(out.join("orientation.md")).expect("read orientation.md");
    assert!(
        orient.contains("## 5F. Foreign API surface (re-exported)"),
        "5F section renders when foreign re-exports exist"
    );
    assert!(
        orient.contains("**`extfut`** (namespace): SinkExt"),
        "foreign root lists its namespace flag + leaf; got:\n{}",
        orient
            .lines()
            .skip_while(|l| !l.starts_with("## 5F."))
            .take(8)
            .collect::<Vec<_>>()
            .join("\n")
    );

    let ctmp = TempDir::new().expect("consumer tempdir");
    let cfiles: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from(
                "[package]\nname=\"consumer\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../t/lib\"}\n",
            ),
        ),
        (
            "src/lib.rs",
            String::from(
                "use lib::{SinkExt, Core};\nuse lib::extfut::FuturesOrdered;\npub fn go(_c: Core) { let _ = Core::new(); }\npub fn t<T: SinkExt>() {}\npub fn h(_f: FuturesOrdered) {}\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(ctmp.path(), &cfiles);

    let out_path = ctmp.path().join("trace_out.json");
    measure_demand(ctmp.path(), &out, Some(&out_path))
        .expect("foreign-reexport demands must not gate");
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&out_path).expect("read --out json"),
    )
    .expect("parse --out json");
    assert_eq!(
        report.pointer("/summary/miss_count").and_then(|v| v.as_u64()),
        Some(0),
        "no gated misses"
    );
    assert_eq!(
        report
            .pointer("/summary/foreign_reexport_count")
            .and_then(|v| v.as_u64()),
        Some(2),
        "SinkExt (leaf) + FuturesOrdered (namespace) bucket as foreign"
    );
    let foreign_names: Vec<&str> = report
        .pointer("/summary/foreign_reexports")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.get("name").and_then(|v| v.as_str()))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        foreign_names.contains(&"SinkExt") && foreign_names.contains(&"FuturesOrdered"),
        "bucket carries both spellings; got {foreign_names:?}"
    );
}

#[test]
fn overlay_paths_serve_as_matcher_aliases() {
    // A pass carrying rustdoc_overlay.json lends its canonical fn
    // paths to the matcher: a demand spelled through a path the
    // structural binding collector cannot see (the glob-re-export
    // class) is served by the rendered pair via the overlay alias.
    let tmp = TempDir::new().expect("target tempdir");
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
            // util2 exists as a real module (the glob-re-export class
            // hides the BINDING, not the module): the demanded outer
            // lands in the mod_namespace bucket while the overlay
            // alias serves the fn + pair.
            "lib/src/lib.rs",
            String::from(
                "pub mod m { pub fn mfn() {} }\npub mod util2 {}\npub struct Core;\nimpl Core { pub fn new() -> Self { Core } }\n",
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
                "pub fn run(_c: lib::Core) { lib::m::mfn(); }\npub fn run2() { lib::m::mfn(); }\n",
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
    let orient = std::fs::read_to_string(out.join("orientation.md")).expect("read orientation");
    assert!(
        orient.contains("`implementation_functions:m::mfn`"),
        "fixture premise: the m::mfn pair renders"
    );
    // Fabricated overlay: rustdoc's canonical path for mfn runs
    // through util2 (the spelling the structural collector cannot
    // bind).
    std::fs::write(
        out.join("rustdoc_overlay.json"),
        "{\"status\":\"ok\",\"paths\":{\"mfn\":[\"lib::util2::mfn\"]}}",
    )
    .expect("write overlay");

    let ctmp = TempDir::new().expect("consumer tempdir");
    let cfiles: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from(
                "[package]\nname=\"consumer\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../t/lib\"}\n",
            ),
        ),
        (
            // The overlay-only spelling: full-path call through
            // util2 (parent joins the demand root set).
            "src/lib.rs",
            String::from("pub fn go() { lib::util2::mfn(); }\n"),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(ctmp.path(), &cfiles);
    let out_path = ctmp.path().join("trace_out.json");
    measure_demand(ctmp.path(), &out, Some(&out_path))
        .expect("overlay-alias spelling is served");
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&out_path).expect("read --out json"),
    )
    .expect("parse --out json");
    assert_eq!(
        report.pointer("/summary/miss_count").and_then(|v| v.as_u64()),
        Some(0),
        "mfn served via the overlay alias"
    );
    assert_eq!(
        report.pointer("/summary/pair_miss_count").and_then(|v| v.as_u64()),
        Some(0),
        "util2::mfn lands exact via the overlay alias tier"
    );
}

#[test]
fn explicit_external_reexport_bypasses_local_shim() {
    // The bevy Write shape: the target carries a local `mod core`
    // shim AND `pub use ::core::fmt::Write;` in the same scope. The
    // leading `::` is the explicit-external marker - the
    // local-module gate must not absorb it, so Write reaches the
    // foreign tiers (5F + the non-gating demand bucket).
    let tmp = TempDir::new().expect("target tempdir");
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
                "mod core { pub fn shim() {} }\npub use ::core::fmt::Write;\npub struct Core2;\nimpl Core2 { pub fn new() -> Self { Core2 } }\n",
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
                "use lib::Core2;\npub fn a() -> Core2 { Core2::new() }\npub fn b() -> Core2 { Core2::new() }\n",
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
    assert!(
        orient.contains("## 5F. Foreign API surface (re-exported)")
            && orient.contains("**`core`**: Write"),
        "explicit-external re-export reaches 5F despite the shim mod; 5F:\n{}",
        orient
            .lines()
            .skip_while(|l| !l.starts_with("## 5F."))
            .take(8)
            .collect::<Vec<_>>()
            .join("\n")
    );

    let ctmp = TempDir::new().expect("consumer tempdir");
    let cfiles: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from(
                "[package]\nname=\"consumer\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nlib={path=\"../t/lib\"}\n",
            ),
        ),
        (
            "src/lib.rs",
            String::from(
                "use lib::{Write, Core2};\npub fn t<T: Write>() {}\npub fn go() -> Core2 { Core2::new() }\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(ctmp.path(), &cfiles);
    let out_path = ctmp.path().join("trace_out.json");
    measure_demand(ctmp.path(), &out, Some(&out_path))
        .expect("foreign-served Write must not gate");
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&out_path).expect("read --out json"),
    )
    .expect("parse --out json");
    assert_eq!(
        report.pointer("/summary/miss_count").and_then(|v| v.as_u64()),
        Some(0),
        "no gated misses"
    );
    let foreign_names: Vec<&str> = report
        .pointer("/summary/foreign_reexports")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.get("name").and_then(|v| v.as_str()))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        foreign_names.contains(&"Write"),
        "Write buckets foreign; got {foreign_names:?}"
    );
}

fn build_nu_named_target(root: &Path) -> std::path::PathBuf {
    // A target whose member crate is literally named `nu` - the
    // collision shape behind the demand-side local-module gate
    // (nu-jupyter-kernel vs nushell's `nu` binary crate).
    let files: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from("[workspace]\nmembers=[\"nu\",\"app\"]\n"),
        ),
        (
            "nu/Cargo.toml",
            String::from(
                "[package]\nname=\"nu\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        (
            "nu/src/lib.rs",
            String::from(
                "pub struct Konst;\nimpl Konst { pub fn new() -> Self { Konst } }\npub fn reg() {}\n",
            ),
        ),
        (
            "app/Cargo.toml",
            String::from(
                "[package]\nname=\"app\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nnu={path=\"../nu\"}\n",
            ),
        ),
        (
            "app/src/lib.rs",
            String::from(
                "use nu::{Konst, reg};\npub fn a() -> Konst { reg(); Konst::new() }\npub fn b() -> Konst { reg(); Konst::new() }\n",
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
    out
}

#[test]
fn consumer_local_module_is_not_target_demand() {
    // The nu-jupyter-kernel shape: the consumer declares its OWN
    // top-level `nu` module and reaches it via uniform paths - a
    // scope-precise import (`use nu::konst::Konst;` beside
    // `pub mod nu;`), a qualified call (`nu::konst::reg()`), and a
    // bare factory call through the file binding (`Konst::new()`).
    // None of it is target demand; the trace must read zero.
    let tmp = TempDir::new().expect("target tempdir");
    let target_out = build_nu_named_target(tmp.path());

    let ctmp = TempDir::new().expect("consumer tempdir");
    let cfiles: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from(
                "[package]\nname=\"kernel\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\n",
            ),
        ),
        (
            // Top-level local module + scope-precise import +
            // qualified call + bare factory call through the file
            // binding.
            "src/lib.rs",
            String::from(
                "pub mod handlers;\npub mod nu;\nuse nu::konst::Konst;\npub fn go() -> Konst { nu::konst::reg(); Konst::new() }\n",
            ),
        ),
        (
            // The handlers/shell.rs shape: the qualifier `nu` here is
            // a crate-rooted module BINDING (`use crate::nu;`), not a
            // crate name - the qualifier-binding resolution must read
            // it as consumer-internal.
            "src/handlers.rs",
            String::from("use crate::nu;\npub fn h() { nu::konst::reg(); }\n"),
        ),
        ("src/nu/mod.rs", String::from("pub mod konst;\n")),
        (
            "src/nu/konst.rs",
            String::from(
                "pub struct Konst;\nimpl Konst { pub fn new() -> Self { Konst } }\npub fn reg() {}\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(ctmp.path(), &cfiles);
    let out_path = ctmp.path().join("trace_out.json");
    measure_demand(ctmp.path(), &target_out, Some(&out_path))
        .expect("a local-module consumer registers no demand");
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&out_path).expect("read --out json"),
    )
    .expect("parse --out json");
    assert_eq!(
        report
            .pointer("/summary/demanded_names")
            .and_then(|v| v.as_u64()),
        Some(0),
        "every nu:: spelling is consumer-local; report: {report}"
    );
    assert_eq!(
        report.pointer("/summary/pairs_total").and_then(|v| v.as_u64()),
        Some(0),
        "no phantom pairs"
    );
}

#[test]
fn real_target_import_still_demands_without_local_module() {
    // Negative control for the local-module gate: same target, a
    // consumer with NO local `nu` module - `use nu::Konst;` is real
    // demand and must register and be covered.
    let tmp = TempDir::new().expect("target tempdir");
    let target_out = build_nu_named_target(tmp.path());

    let ctmp = TempDir::new().expect("consumer tempdir");
    let cfiles: HashMap<&str, String> = [
        (
            "Cargo.toml",
            String::from(
                "[package]\nname=\"consumer\"\nversion=\"0.0.1\"\nedition=\"2021\"\n[dependencies]\nnu={path=\"../t/nu\"}\n",
            ),
        ),
        (
            // Direct import PLUS a renamed namespace import whose
            // qualified call must resolve through the binding
            // (`use nu as nucrate; nucrate::reg()` is real target
            // demand under the qualifier-binding resolution).
            "src/lib.rs",
            String::from(
                "use nu as nucrate;\nuse nu::Konst;\npub fn go() -> Konst { Konst::new() }\npub fn go2() { nucrate::reg(); }\n",
            ),
        ),
    ]
    .into_iter()
    .collect();
    write_tree(ctmp.path(), &cfiles);
    let out_path = ctmp.path().join("trace_out.json");
    measure_demand(ctmp.path(), &target_out, Some(&out_path))
        .expect("real demand is covered by the target's picks");
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&out_path).expect("read --out json"),
    )
    .expect("parse --out json");
    let hit_names: Vec<&str> = report
        .get("hits")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.get("name").and_then(|v| v.as_str()))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        hit_names.contains(&"Konst"),
        "no-local-module consumer still demands the target; hits: {hit_names:?}"
    );
    assert!(
        hit_names.contains(&"reg"),
        "renamed-namespace qualified call resolves through the binding; hits: {hit_names:?}"
    );
}
