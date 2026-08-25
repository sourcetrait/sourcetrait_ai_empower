//! In-process rig lifecycle (new / uninstall) against the namespace on disk. The
//! FRESH-PROCESS substrate init (keypair + repo creation at startup) is a SYSTEM
//! concern -> `sourcetrait_grammar_tests` (substrate.rs).

use std::path::Path;
use std::process::Command;

use sourcetrait_grammar_mcp::guts::{TestServer, has_error, valid_function_source, write_source};
use sourcetrait_common::testing::prelude::*;

/// One shared in-process server per test binary: constructing a TestServer runs
/// the namespace substrate (keypair, rigs repo git config), which must not race
/// itself across parallel tests.
static TESTING: testing::ModuleWith<TestServer> = testing::module_with!(Integration, {
    .using_temp_dir()
    .setup(|_| TestServer::new())
});

/// Commit-log subjects in the in-process namespace's rigs repo (read-only).
fn git_log_subjects(repo: &Path) -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("log")
        .arg("--pretty=%s")
        .output()
        .expect("git log");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect()
}

#[tested]
fn rig_new_writes_repo_and_records_meta() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TESTING.harness();
    let src = t.temp_dir().join("mylib");
    let env = s.rig("new", "sourcetrait/mylib", src.to_str().unwrap());
    assert!(!has_error(&env), "rig(new) should succeed; got {env}");
    let lib_dir = s.rig_dir("sourcetrait/mylib");
    assert!(lib_dir.exists(), "lib dir should exist at {}", lib_dir.display());
    assert!(lib_dir.join("mod.nu").exists(), "lib mod.nu should exist");
    let meta_path = lib_dir.join(".meta/rig.nuon");
    assert!(meta_path.exists(), "the meta sidecar should exist, and be NUON");
    let meta = s.rig_index("sourcetrait/mylib");
    assert_eq!(meta["source_path"].as_str(), src.to_str());
    assert!(
        meta.get("kind").is_none(),
        "meta should not carry a kind field; got {meta}",
    );
    assert!(src.exists(), "source dir should exist");
    assert!(src.join("mod.nu").exists(), "source root mod.nu should be seeded");
    let log = git_log_subjects(&s.rigs_dir());
    assert!(
        log.iter().any(|l| l == "new rig sourcetrait/mylib"),
        "expected new-rig commit in log; got {log:?}",
    );
}

#[tested]
fn uninstall_removes_subtree_keeps_source() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TESTING.harness();
    let src = t.temp_dir().join("droppable");
    let _ = s.rig("new", "sourcetrait/droppable", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let _ = s.commit("sourcetrait/droppable");
    assert!(s.rig_dir("sourcetrait/droppable").exists());
    let env = s.rig("uninstall", "sourcetrait/droppable", src.to_str().unwrap());
    assert!(!has_error(&env), "uninstall should succeed; got {env}");
    assert!(
        !s.rig_dir("sourcetrait/droppable").exists(),
        "lib dir should be gone after uninstall",
    );
    assert!(src.exists(), "source should remain after uninstall");
    assert!(
        src.join("m/thing/mod.nu").exists(),
        "source files should remain after uninstall",
    );
    let log = git_log_subjects(&s.rigs_dir());
    assert!(
        log.iter().any(|l| l == "uninstall rig sourcetrait/droppable"),
        "expected uninstall-rig commit in log; got {log:?}",
    );
}

#[test]
fn uninstall_missing_succeeds() {
    let s = TESTING.harness();
    let env = s.rig("uninstall", "sourcetrait/neverexisted", "/some/path");
    assert!(
        !has_error(&env),
        "uninstall of unknown lib should be idempotent success; got {env}",
    );
}
