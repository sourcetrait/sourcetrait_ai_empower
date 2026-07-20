//! In-process library lifecycle (new / uninstall) against the store on disk. The
//! FRESH-PROCESS substrate init (keypair + repo creation at startup) is a SYSTEM
//! concern -> `sourcetrait_grammar_tests` (substrate.rs).

use std::path::Path;
use std::process::Command;

use sourcetrait_grammar_mcp::guts::{TestServer, has_error, valid_function_source, write_source};
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

/// Commit-log subjects in the in-process store's libraries repo (read-only).
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

#[test]
#[named]
fn library_new_writes_repo_and_records_meta() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("mylib");
    let env = s.library("new", "sourcetrait/mylib", src.to_str().unwrap());
    assert!(!has_error(&env), "library(new) should succeed; got {env}");
    let lib_dir = s.library_dir("sourcetrait/mylib");
    assert!(lib_dir.exists(), "lib dir should exist at {}", lib_dir.display());
    assert!(lib_dir.join("mod.nu").exists(), "lib mod.nu should exist");
    let meta_path = lib_dir.join(".meta/library.json");
    assert!(meta_path.exists(), "meta sidecar should exist");
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&meta_path).expect("read meta")).expect("decode meta");
    assert_eq!(meta["source_path"].as_str(), src.to_str());
    assert!(
        meta.get("kind").is_none(),
        "meta should not carry a kind field; got {meta}",
    );
    assert!(src.exists(), "source dir should exist");
    assert!(src.join("mod.nu").exists(), "source root mod.nu should be seeded");
    let log = git_log_subjects(&s.libraries_dir());
    assert!(
        log.iter().any(|l| l == "new library sourcetrait/mylib"),
        "expected new-library commit in log; got {log:?}",
    );
}

#[test]
#[named]
fn uninstall_removes_subtree_keeps_source() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("droppable");
    let _ = s.library("new", "sourcetrait/droppable", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use thing\n");
    write_source(
        &src,
        "m/thing/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let _ = s.commit("sourcetrait/droppable");
    assert!(s.library_dir("sourcetrait/droppable").exists());
    let env = s.library("uninstall", "sourcetrait/droppable", src.to_str().unwrap());
    assert!(!has_error(&env), "uninstall should succeed; got {env}");
    assert!(
        !s.library_dir("sourcetrait/droppable").exists(),
        "lib dir should be gone after uninstall",
    );
    assert!(src.exists(), "source should remain after uninstall");
    assert!(
        src.join("m/thing/mod.nu").exists(),
        "source files should remain after uninstall",
    );
    let log = git_log_subjects(&s.libraries_dir());
    assert!(
        log.iter().any(|l| l == "uninstall library sourcetrait/droppable"),
        "expected uninstall-library commit in log; got {log:?}",
    );
}

#[test]
fn uninstall_missing_succeeds() {
    let s = TestServer::new();
    let env = s.library("uninstall", "sourcetrait/neverexisted", "/some/path");
    assert!(
        !has_error(&env),
        "uninstall of unknown lib should be idempotent success; got {env}",
    );
}
