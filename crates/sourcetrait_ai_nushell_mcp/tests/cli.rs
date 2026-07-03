//! One-shot human CLI tests (`nushell_mcp cli <tool> ...`).
//!
//! Verifies:
//!   - `cli info` prints the envelope as NUON and exits 0;
//!   - the library lifecycle (library new -> commit -> call with a NUON
//!     args record) works end to end as plain subprocesses;
//!   - `cli run` accepts NUON schemas/args and evaluates;
//!   - an error envelope exits 1 (and still prints the envelope);
//!   - `cli kill` (no-return tool) prints nothing and exits 0.

use std::path::Path;
use std::process::{Command, Output};

/// Run `nushell_mcp <args...>` one-shot against the given XDG dirs.
fn cli(args: &[&str], data: &Path, cache: &Path) -> Output {
    let host_bin = env!("CARGO_BIN_EXE_nushell_mcp");
    let worker_bin = env!("CARGO_BIN_EXE_nushell_mcp_worker");
    Command::new(host_bin)
        .args(args)
        .env("NUSHELL_MCP_WORKER_PATH", worker_bin)
        .env("XDG_DATA_HOME", data)
        .env("XDG_CACHE_HOME", cache)
        .output()
        .expect("run nushell_mcp cli")
}

fn stdout_str(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn write_source(dir: &Path, rel: &str, contents: &str) {
    let target = dir.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(&target, contents).expect("write source");
}

#[test]
fn cli_info_prints_nuon() {
    let data = tempfile::tempdir().expect("data");
    let cache = tempfile::tempdir().expect("cache");
    let out = cli(
        &["--id", "cid", "cli", "info"],
        data.path(),
        cache.path(),
    );
    assert!(out.status.success(), "cli info should exit 0; got {out:?}");
    let text = stdout_str(&out);
    assert!(
        text.contains("nushell_mcp"),
        "info NUON should carry the name; got {text:?}",
    );
    assert!(
        text.contains("nu_version"),
        "info NUON should carry nu_version; got {text:?}",
    );
    assert!(
        text.contains("cid"),
        "info NUON should carry the configured id; got {text:?}",
    );
}

#[test]
fn cli_library_lifecycle_and_call() {
    let data = tempfile::tempdir().expect("data");
    let cache = tempfile::tempdir().expect("cache");
    let src_root = tempfile::tempdir().expect("src");
    let src = src_root.path().join("clilib");
    let src_str = src.to_str().unwrap();

    let established = cli(
        &[
            "--id",
            "cid",
            "cli",
            "library",
            "new",
            "sourcetrait/clilib",
            src_str,
        ],
        data.path(),
        cache.path(),
    );
    assert!(
        established.status.success(),
        "cli library new should exit 0; got {established:?}",
    );

    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export module double\n");
    write_source(
        &src,
        "m/double/mod.nu",
        "export def main [args: record<x: int>]: nothing -> record<out: int> {\n{ out: ($args.x * 2) }\n}\n",
    );

    let committed = cli(
        &["--id", "cid", "cli", "commit", "sourcetrait/clilib"],
        data.path(),
        cache.path(),
    );
    assert!(
        committed.status.success(),
        "cli commit should exit 0; got stdout={:?} stderr={:?}",
        stdout_str(&committed),
        String::from_utf8_lossy(&committed.stderr),
    );

    let called = cli(
        &[
            "--id",
            "cid",
            "cli",
            "call",
            "sourcetrait/clilib:m:double",
            "{x: 21}",
        ],
        data.path(),
        cache.path(),
    );
    assert!(
        called.status.success(),
        "cli call should exit 0; got stdout={:?} stderr={:?}",
        stdout_str(&called),
        String::from_utf8_lossy(&called.stderr),
    );
    let text = stdout_str(&called);
    assert!(
        text.contains("out") && text.contains("42"),
        "call envelope should carry out: 42; got {text:?}",
    );
}

#[test]
fn cli_run_evaluates_nuon_schemas_and_args() {
    let data = tempfile::tempdir().expect("data");
    let cache = tempfile::tempdir().expect("cache");
    let out = cli(
        &[
            "--id",
            "cid",
            "cli",
            "run",
            "--args-schema",
            "{x: int}",
            "--result-schema",
            "{out: int}",
            "--args",
            "{x: 5}",
            "{ out: ($args.x + 1) }",
        ],
        data.path(),
        cache.path(),
    );
    assert!(
        out.status.success(),
        "cli run should exit 0; got stdout={:?} stderr={:?}",
        stdout_str(&out),
        String::from_utf8_lossy(&out.stderr),
    );
    let text = stdout_str(&out);
    assert!(
        text.contains("out") && text.contains('6'),
        "run envelope should carry out: 6; got {text:?}",
    );
}

#[test]
fn cli_error_envelope_exits_one() {
    let data = tempfile::tempdir().expect("data");
    let cache = tempfile::tempdir().expect("cache");
    let out = cli(
        &[
            "--id",
            "cid",
            "cli",
            "call",
            "sourcetrait/ghost:m:noop",
            "{}",
        ],
        data.path(),
        cache.path(),
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "an error envelope should exit 1; got {out:?}",
    );
    let text = stdout_str(&out);
    assert!(
        text.contains("error"),
        "the error envelope should still print; got {text:?}",
    );
}

#[test]
fn cli_kill_prints_nothing_and_exits_zero() {
    let data = tempfile::tempdir().expect("data");
    let cache = tempfile::tempdir().expect("cache");
    let out = cli(
        &["--id", "cid", "cli", "kill", "doesnotexist"],
        data.path(),
        cache.path(),
    );
    assert!(out.status.success(), "cli kill should exit 0; got {out:?}");
    assert!(
        stdout_str(&out).trim().is_empty(),
        "no-return tools print nothing; got {:?}",
        stdout_str(&out),
    );
}
