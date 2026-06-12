//! call() tests for 0.0.13 -- the final tool in slice 3.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
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
    source_root: tempfile::TempDir,
}

impl Host {
    fn spawn() -> Self {
        let host_bin = env!("CARGO_BIN_EXE_nushell_mcp");
        let worker_bin = env!("CARGO_BIN_EXE_nushell_mcp_worker");
        let data_dir = tempfile::tempdir().expect("data tempdir");
        let cache_dir = tempfile::tempdir().expect("cache tempdir");
        let client_mirror_root = tempfile::tempdir().expect("client mirror tempdir");
        let source_root = tempfile::tempdir().expect("source tempdir");
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
            source_root,
        };
        host.initialize();
        host
    }

    fn client_dir(&self, name: &str) -> PathBuf {
        self.client_mirror_root.path().join(name)
    }

    fn source_dir(&self, name: &str) -> PathBuf {
        self.source_root.path().join(name)
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
                "clientInfo": {"name": "library_call", "version": "0.0.1"}
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

    fn call_tool(&mut self, tool: &str, args: serde_json::Value) -> serde_json::Value {
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

fn extract_envelope(resp: &serde_json::Value) -> Option<serde_json::Value> {
    // Success envelopes are inside structuredContent at the top level
    // (no `error` key). Error envelopes have `error`; this helper is
    // for the success path. Returns None when an error envelope sits
    // there instead.
    let sc = resp.get("result")?.get("structuredContent")?.clone();
    if sc.get("error").is_some() {
        return None;
    }
    Some(sc)
}

fn write_source(dir: &Path, rel: &str, contents: &str) {
    let target = dir.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(target, contents).unwrap();
}

#[test]
fn call_after_define_returns_result() {
    let mut host = Host::spawn();
    let mirror = host.client_dir("calc");
    let _ = host.call_tool(
        "register_library",
        serde_json::json!({
            "name": "calc",
            "path": mirror.to_str().unwrap(),
        }),
    );
    let _ = host.call_tool(
        "define_function",
        serde_json::json!({
            "library": "calc",
            "module_path": "math",
            "name": "double",
            "args_schema": "x: int",
            "result_schema": "out: int",
            "body": "{ out: ($args.x * 2) }",
        }),
    );
    let resp = host.call_tool(
        "call",
        serde_json::json!({
            "library": "calc",
            "module_path": "math",
            "name": "double",
            "args": {"x": 7},
        }),
    );
    let env = extract_envelope(&resp).unwrap_or_else(|| panic!("call envelope; got {resp}"));
    assert_eq!(env["result"]["out"].as_i64(), Some(14));
    // No rerun_id, no version_id (HEAD-only).
    assert!(env.get("rerun_id").is_none(), "call envelope shouldn't echo rerun_id");
    assert!(env.get("version_id").is_none(), "call envelope shouldn't echo version_id");
}

#[test]
fn call_after_import_returns_result() {
    let mut host = Host::spawn();
    let src = host.source_dir("importable");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "");
    write_source(
        &src,
        "triple.nu",
        "export def main [args: record<x: int>] {\n    { out: ($args.x * 3) }\n}\n\nexport def resolve [args: record<out: int>] {\n    $args\n}\n",
    );
    let _ = host.call_tool(
        "import_library",
        serde_json::json!({
            "name": "importable",
            "path": src.to_str().unwrap(),
        }),
    );
    let resp = host.call_tool(
        "call",
        serde_json::json!({
            "library": "importable",
            "module_path": "",
            "name": "triple",
            "args": {"x": 11},
        }),
    );
    let env = extract_envelope(&resp).unwrap_or_else(|| panic!("call envelope; got {resp}"));
    assert_eq!(env["result"]["out"].as_i64(), Some(33));
}

#[test]
fn call_unknown_library_errors() {
    let mut host = Host::spawn();
    let resp = host.call_tool(
        "call",
        serde_json::json!({
            "library": "ghost",
            "module_path": "",
            "name": "noop",
            "args": {"noop": 0},
        }),
    );
    assert!(has_error_path(&resp));
}

#[test]
fn call_missing_function_errors() {
    let mut host = Host::spawn();
    let mirror = host.client_dir("partlib");
    let _ = host.call_tool(
        "register_library",
        serde_json::json!({
            "name": "partlib",
            "path": mirror.to_str().unwrap(),
        }),
    );
    let resp = host.call_tool(
        "call",
        serde_json::json!({
            "library": "partlib",
            "module_path": "",
            "name": "ghost",
            "args": {"noop": 0},
        }),
    );
    assert!(has_error_path(&resp));
}

#[test]
fn call_bad_module_path_errors() {
    let mut host = Host::spawn();
    let mirror = host.client_dir("safelib");
    let _ = host.call_tool(
        "register_library",
        serde_json::json!({
            "name": "safelib",
            "path": mirror.to_str().unwrap(),
        }),
    );
    for bad in ["../etc", "a/../b", "/abs"] {
        let resp = host.call_tool(
            "call",
            serde_json::json!({
                "library": "safelib",
                "module_path": bad,
                "name": "x",
                "args": {"n": 0},
            }),
        );
        assert!(has_error_path(&resp), "module_path {bad:?} should error; got {resp}");
    }
}

#[test]
fn call_args_typecheck_failure_surfaces() {
    let mut host = Host::spawn();
    let mirror = host.client_dir("strictlib");
    let _ = host.call_tool(
        "register_library",
        serde_json::json!({
            "name": "strictlib",
            "path": mirror.to_str().unwrap(),
        }),
    );
    let _ = host.call_tool(
        "define_function",
        serde_json::json!({
            "library": "strictlib",
            "module_path": "",
            "name": "needs_int",
            "args_schema": "x: int",
            "result_schema": "out: int",
            "body": "{ out: $args.x }",
        }),
    );
    // Send a string where int is expected; worker should reject at parse time.
    let resp = host.call_tool(
        "call",
        serde_json::json!({
            "library": "strictlib",
            "module_path": "",
            "name": "needs_int",
            "args": {"x": "five"},
        }),
    );
    assert!(has_error_path(&resp), "type mismatch should surface as error; got {resp}");
}
