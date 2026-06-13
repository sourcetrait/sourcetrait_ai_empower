//! define_function / undefine_function tests for 0.0.11.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Host {
    child: Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
    #[allow(dead_code)]
    data_dir: tempfile::TempDir,
    #[allow(dead_code)]
    cache_dir: tempfile::TempDir,
    client_mirror_root: tempfile::TempDir,
}

impl Host {
    fn spawn() -> Self {
        let host_bin = env!("CARGO_BIN_EXE_nushell_mcp");
        let worker_bin = env!("CARGO_BIN_EXE_nushell_mcp_worker");
        let data_dir = tempfile::tempdir().expect("data tempdir");
        let cache_dir = tempfile::tempdir().expect("cache tempdir");
        let client_mirror_root = tempfile::tempdir().expect("client mirror tempdir");
        let mut child = Command::new(host_bin)
            .env("NUSHELL_MCP_WORKER_PATH", worker_bin)
            .env("XDG_DATA_HOME", data_dir.path())
            .env("XDG_CACHE_HOME", cache_dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn host");
        let stdin = child.stdin.take().expect("host stdin");
        let stdout = BufReader::new(child.stdout.take().expect("host stdout"));
        let mut host = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
            data_dir,
            cache_dir,
            client_mirror_root,
        };
        host.initialize();
        host
    }

    fn libraries_dir(&self) -> PathBuf {
        self.data_dir
            .path()
            .join("sourcetrait")
            .join("nushell_mcp")
            .join("libraries")
    }

    fn library_dir(&self, name: &str) -> PathBuf {
        self.libraries_dir().join(name)
    }

    fn client_dir(&self, name: &str) -> PathBuf {
        self.client_mirror_root.path().join(name)
    }

    fn initialize(&mut self) {
        let id = self.next_id();
        let init = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "library_define", "version": "0.0.1"}
            }
        });
        self.send(&init);
        let _ = self.read_id(id);
        let initialized = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        });
        self.send(&initialized);
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn send(&mut self, msg: &serde_json::Value) {
        let line = msg.to_string();
        self.stdin.write_all(line.as_bytes()).expect("write line");
        self.stdin.write_all(b"\n").expect("write newline");
        self.stdin.flush().expect("flush");
    }

    fn read_id(&mut self, expected_id: u64) -> serde_json::Value {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if Instant::now() >= deadline {
                panic!("timed out waiting for response id {expected_id}");
            }
            let mut line = String::new();
            let n = self.stdout.read_line(&mut line).expect("read line");
            if n == 0 {
                panic!("EOF on host stdout waiting for id {expected_id}");
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let msg: serde_json::Value = serde_json::from_str(trimmed)
                .unwrap_or_else(|e| panic!("parse JSON: {e} from {trimmed:?}"));
            if msg.get("id").and_then(|v| v.as_u64()) == Some(expected_id) {
                return msg;
            }
        }
    }

    fn call(&mut self, tool: &str, args: serde_json::Value) -> serde_json::Value {
        let id = self.next_id();
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": tool, "arguments": args}
        });
        self.send(&req);
        self.read_id(id)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn has_error_path(resp: &serde_json::Value) -> bool {
    resp.get("result")
        .and_then(|r| r.get("structuredContent"))
        .and_then(|sc| sc.get("error"))
        .is_some()
}

#[test]
fn define_writes_file_cascade_and_mirror() {
    let mut host = Host::spawn();
    let client_dir = host.client_dir("mathlib");
    let _ = host.call(
        "register_library",
        serde_json::json!({
            "name": "mathlib",
            "path": client_dir.to_str().expect("client_dir to str"),
        }),
    );
    let resp = host.call(
        "define_function",
        serde_json::json!({
            "library": "mathlib",
            "module_path": "math",
            "name": "double",
            "args_schema": {"x": "int"},
            "result_schema": {"out": "int"},
            "body": "{ out: ($args.x * 2) }",
        }),
    );
    assert!(!has_error_path(&resp), "define should succeed; got {resp}");
    let lib = host.library_dir("mathlib");
    let target = lib.join("math").join("double.nu");
    assert!(target.exists(), "function file should exist at {}", target.display());
    let src = std::fs::read_to_string(&target).expect("read func");
    assert!(src.contains("export def main"), "main not in {src:?}");
    assert!(src.contains("export def resolve"), "resolve not in {src:?}");
    // Cascade: library root mod.nu has `export module math`; math/mod.nu has
    // `export use ./double.nu`.
    let root_mod = std::fs::read_to_string(lib.join("mod.nu")).expect("read root mod.nu");
    assert!(
        root_mod.contains("export module math"),
        "root mod.nu should re-export math; got {root_mod:?}",
    );
    let math_mod = std::fs::read_to_string(lib.join("math").join("mod.nu"))
        .expect("read math mod.nu");
    assert!(
        math_mod.contains("export use ./double.nu"),
        "math mod.nu should export double; got {math_mod:?}",
    );
    // Mirror has the same shape.
    let mirror_target = client_dir.join("math").join("double.nu");
    assert!(mirror_target.exists(), "mirror function should exist");
    let mirror_root = std::fs::read_to_string(client_dir.join("mod.nu")).expect("read mirror root");
    assert!(mirror_root.contains("export module math"), "mirror cascade missing");
}

#[test]
fn undefine_removes_and_prunes() {
    let mut host = Host::spawn();
    let client_dir = host.client_dir("droplib");
    let _ = host.call(
        "register_library",
        serde_json::json!({
            "name": "droplib",
            "path": client_dir.to_str().expect("client_dir to str"),
        }),
    );
    let _ = host.call(
        "define_function",
        serde_json::json!({
            "library": "droplib",
            "module_path": "deep/path",
            "name": "victim",
            "args_schema": {"x": "int"},
            "result_schema": {"out": "int"},
            "body": "{ out: $args.x }",
        }),
    );
    let lib = host.library_dir("droplib");
    assert!(lib.join("deep").join("path").join("victim.nu").exists());
    let resp = host.call(
        "undefine_function",
        serde_json::json!({
            "library": "droplib",
            "module_path": "deep/path",
            "name": "victim",
        }),
    );
    assert!(!has_error_path(&resp), "undefine should succeed; got {resp}");
    // File gone.
    assert!(!lib.join("deep").join("path").join("victim.nu").exists());
    // Empty intermediate dirs pruned all the way back.
    assert!(!lib.join("deep").exists(), "empty deep should be pruned");
    // Root mod.nu is now empty (no submodules left).
    let root_mod = std::fs::read_to_string(lib.join("mod.nu")).expect("read root mod.nu");
    assert!(
        !root_mod.contains("deep"),
        "root mod.nu should not reference deep anymore; got {root_mod:?}",
    );
    // Mirror followed.
    assert!(!client_dir.join("deep").exists(), "mirror deep should be pruned");
}

#[test]
fn define_overwrites_existing() {
    let mut host = Host::spawn();
    let client_dir = host.client_dir("overlib");
    let _ = host.call(
        "register_library",
        serde_json::json!({
            "name": "overlib",
            "path": client_dir.to_str().expect("client_dir to str"),
        }),
    );
    let _ = host.call(
        "define_function",
        serde_json::json!({
            "library": "overlib",
            "module_path": "",
            "name": "thing",
            "args_schema": {"x": "int"},
            "result_schema": {"out": "int"},
            "body": "{ out: 1 }",
        }),
    );
    let _ = host.call(
        "define_function",
        serde_json::json!({
            "library": "overlib",
            "module_path": "",
            "name": "thing",
            "args_schema": {"x": "int"},
            "result_schema": {"out": "int"},
            "body": "{ out: 2 }",
        }),
    );
    let src = std::fs::read_to_string(host.library_dir("overlib").join("thing.nu"))
        .expect("read func");
    assert!(src.contains("{ out: 2 }"), "should have second body; got {src:?}");
}

#[test]
fn multi_function_same_dir_updates_cascade() {
    let mut host = Host::spawn();
    let client_dir = host.client_dir("multilib");
    let _ = host.call(
        "register_library",
        serde_json::json!({
            "name": "multilib",
            "path": client_dir.to_str().expect("client_dir to str"),
        }),
    );
    for fname in ["alpha", "beta", "gamma"] {
        let _ = host.call(
            "define_function",
            serde_json::json!({
                "library": "multilib",
                "module_path": "ops",
                "name": fname,
                "args_schema": {"x": "int"},
                "result_schema": {"out": "int"},
                "body": "{ out: $args.x }",
            }),
        );
    }
    let ops_mod = std::fs::read_to_string(
        host.library_dir("multilib").join("ops").join("mod.nu"),
    )
    .expect("read ops mod.nu");
    for fname in ["alpha", "beta", "gamma"] {
        assert!(
            ops_mod.contains(&format!("export use ./{fname}.nu")),
            "ops mod.nu should re-export {fname}; got {ops_mod:?}",
        );
    }
}

#[test]
fn define_unknown_library_errors() {
    let mut host = Host::spawn();
    let resp = host.call(
        "define_function",
        serde_json::json!({
            "library": "ghost",
            "module_path": "",
            "name": "noop",
            "args_schema": {"noop": "int"},
            "result_schema": {"out": "int"},
            "body": "{ out: 0 }",
        }),
    );
    assert!(has_error_path(&resp), "unknown library should error; got {resp}");
}

#[test]
fn undefine_missing_function_errors() {
    let mut host = Host::spawn();
    let client_dir = host.client_dir("emptylib");
    let _ = host.call(
        "register_library",
        serde_json::json!({
            "name": "emptylib",
            "path": client_dir.to_str().expect("client_dir to str"),
        }),
    );
    let resp = host.call(
        "undefine_function",
        serde_json::json!({
            "library": "emptylib",
            "module_path": "",
            "name": "ghost",
        }),
    );
    assert!(has_error_path(&resp), "missing func should error; got {resp}");
}

#[test]
fn path_traversal_rejected() {
    let mut host = Host::spawn();
    let client_dir = host.client_dir("safelib");
    let _ = host.call(
        "register_library",
        serde_json::json!({
            "name": "safelib",
            "path": client_dir.to_str().expect("client_dir to str"),
        }),
    );
    for bad in ["..", "../etc", "a/../b", "/abs", "trailing/"] {
        let resp = host.call(
            "define_function",
            serde_json::json!({
                "library": "safelib",
                "module_path": bad,
                "name": "x",
                "args_schema": {"n": "int"},
                "result_schema": {"out": "int"},
                "body": "{ out: 0 }",
            }),
        );
        assert!(
            has_error_path(&resp),
            "module_path {bad:?} should be rejected; got {resp}",
        );
    }
}

#[test]
fn define_rejects_syntactically_broken_body() {
    // Slice 6.0 fix 1: lint_body silently returns empty when wrapper
    // parse fails, so without parse_check_function_source a broken body
    // would be committed to disk + cascade + signed commit and only
    // surface at call() time. With the fix, broken bodies are rejected
    // at the seam with -32602 invalid_params.
    let mut host = Host::spawn();
    let client_dir = host.client_dir("brokenlib");
    let _ = host.call(
        "register_library",
        serde_json::json!({
            "name": "brokenlib",
            "path": client_dir.to_str().expect("client_dir to str"),
        }),
    );
    let resp = host.call(
        "define_function",
        serde_json::json!({
            "library": "brokenlib",
            "module_path": "",
            "name": "broken",
            "args_schema": {"x": "int"},
            "result_schema": {"out": "int"},
            // `let z =` is an incomplete let assignment (no rhs); the
            // synthesized function source parses to a parse error.
            // (`let z` without `=` actually parses cleanly in nushell;
            // confirmed via library.rs unit probe.)
            "body": "let z =",
        }),
    );
    assert!(has_error_path(&resp), "broken body should error; got {resp}");
    let msg = resp.to_string();
    assert!(
        msg.contains("parse error"),
        "error message should mention parse error; got {msg}",
    );
    // No file written, no commit landed.
    assert!(
        !host.library_dir("brokenlib").join("broken.nu").exists(),
        "broken function file should not exist on disk",
    );
}

#[test]
fn standalone_driver_invokes_defined_function() {
    let mut host = Host::spawn();
    let client_dir = host.client_dir("drvlib");
    let _ = host.call(
        "register_library",
        serde_json::json!({
            "name": "drvlib",
            "path": client_dir.to_str().expect("client_dir to str"),
        }),
    );
    let _ = host.call(
        "define_function",
        serde_json::json!({
            "library": "drvlib",
            "module_path": "math",
            "name": "double",
            "args_schema": {"x": "int"},
            "result_schema": {"out": "int"},
            "body": "{ out: ($args.x * 2) }",
        }),
    );
    // Spawn `nu` with NU_LIB_DIRS pointing at the MCP repo. Verify:
    //   use drvlib; drvlib math double resolve (drvlib math double {x: 5}) -> {out: 10}
    let out = Command::new("nu")
        .env("NU_LIB_DIRS", host.libraries_dir())
        .arg("-c")
        .arg("use drvlib; drvlib math double resolve (drvlib math double {x: 5}) | to nuon")
        .output()
        .expect("spawn nu");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "nu failed: stderr={:?} stdout={:?}",
        String::from_utf8_lossy(&out.stderr),
        stdout,
    );
    assert!(
        stdout.contains("out:") && stdout.contains("10"),
        "expected {{out: 10}}; got stdout={stdout:?}",
    );
}
