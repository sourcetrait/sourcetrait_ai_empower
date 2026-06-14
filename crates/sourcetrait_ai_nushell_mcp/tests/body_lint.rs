//! Slice 5.1 + 5.2 integration tests for the AST body linter. Each
//! handler that accepts agent-authored body source short-circuits on
//! lint violations with the agent-fixable `lint::<class> [L:C]`
//! report shape (with optional ` mod <rel_path>` source tag for
//! library import paths). Helper-function lint lands in slice 5.3 --
//! not covered here. `rerun()` does NOT re-lint per the_user
//! 2026-05-31 design call (trust the cache).

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
    source_root: tempfile::TempDir,
}

impl Host {
    fn spawn() -> Self {
        let host_bin = env!("CARGO_BIN_EXE_nushell_mcp");
        let worker_bin = env!("CARGO_BIN_EXE_nushell_mcp_worker");
        let data_dir = tempfile::tempdir().expect("data tempdir");
        let cache_dir = tempfile::tempdir().expect("cache tempdir");
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
            source_root,
        };
        host.initialize();
        host
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
                "clientInfo": {"name": "body_lint", "version": "0.0.1"}
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

    fn run(&mut self, args: serde_json::Value) -> serde_json::Value {
        self.call_tool("run", args)
    }

    fn interact(&mut self, args: serde_json::Value) -> serde_json::Value {
        self.call_tool("interact", args)
    }

    fn register(&mut self, name: &str, path: &str) -> serde_json::Value {
        self.call_tool("register_library", serde_json::json!({
            "name": name,
            "path": path,
        }))
    }

    fn define_function(&mut self, args: serde_json::Value) -> serde_json::Value {
        self.call_tool("define_function", args)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn envelope_error<'a>(resp: &'a serde_json::Value) -> Option<&'a serde_json::Value> {
    resp.get("result")?.get("structuredContent")?.get("error")
}

fn has_envelope_error(resp: &serde_json::Value) -> bool {
    envelope_error(resp).is_some()
}

fn envelope_error_kind(resp: &serde_json::Value) -> Option<&str> {
    envelope_error(resp)?.get("kind")?.as_str()
}

fn lint_violation_kinds(resp: &serde_json::Value) -> Vec<String> {
    envelope_error(resp)
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("violations"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter()
            .filter_map(|v| v.get("kind").and_then(|k| k.as_str()).map(str::to_string))
            .collect())
        .unwrap_or_default()
}

#[test]
fn lint_rejects_closure_with_hardcoded_path() {
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": {"noop": "int"},
        "result_schema": {"out": "int"},
        "args": {"noop": 0},
        "body": "{ p: \"/home/box/proj/x\", out: 0 }",
    }));
    assert_eq!(envelope_error_kind(&resp), Some("lint::violations"), "got {resp}");
    let kinds = lint_violation_kinds(&resp);
    assert!(kinds.iter().any(|k| k == "hardcoded_variable"),
        "expected hardcoded_variable in violations; got {kinds:?}");
}

#[test]
fn lint_rejects_closure_with_denied_external() {
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": {"noop": "int"},
        "result_schema": {"out": "int"},
        "args": {"noop": 0},
        "body": "{ x: (^awk '{print $1}' | str trim), out: 0 }",
    }));
    assert_eq!(envelope_error_kind(&resp), Some("lint::violations"), "got {resp}");
    let kinds = lint_violation_kinds(&resp);
    assert!(kinds.iter().any(|k| k == "denied_command"),
        "expected denied_command in violations; got {kinds:?}");
}

#[test]
fn lint_passes_clean_closure() {
    // No lint violations -> reaches the worker -> normal envelope path.
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": {"x": "int"},
        "result_schema": {"out": "int"},
        "args": {"x": 5},
        "body": "{ out: ($args.x + 1) }",
    }));
    // C2: `run` emits structured_content only (no content[] mirror).
    let result = resp.get("result").unwrap_or_else(|| {
        panic!("expected ok result; got {resp}");
    });
    let env = result
        .get("structuredContent")
        .cloned()
        .unwrap_or_else(|| panic!("expected structuredContent; got {resp}"));
    assert_eq!(env["result"]["out"].as_i64(), Some(6), "got {env}");
}

#[test]
fn lint_aggregates_multiple_violations() {
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": {"noop": "int"},
        "result_schema": {"out": "int"},
        "args": {"noop": 0},
        "body": "\
^awk 'x'
cd \"/a/b\"
{ out: 0 }",
    }));
    assert_eq!(envelope_error_kind(&resp), Some("lint::violations"), "got {resp}");
    let kinds = lint_violation_kinds(&resp);
    assert!(kinds.iter().any(|k| k == "denied_command"), "got {kinds:?}");
    assert!(kinds.iter().any(|k| k == "hardcoded_variable"), "got {kinds:?}");
}

// ----------------------------------------------------------------------------
// Slice 5.2: interact() body lint
// ----------------------------------------------------------------------------

#[test]
fn lint_interact_rejects_hardcoded_path() {
    let mut host = Host::spawn();
    let resp = host.interact(serde_json::json!({
        "args_schema": {"noop": "int"},
        "result_schema": {"out": "int"},
        "args": {"noop": 0},
        "body": "{ p: \"/home/box/x\", out: 0 }",
    }));
    assert_eq!(envelope_error_kind(&resp), Some("lint::violations"), "got {resp}");
    let kinds = lint_violation_kinds(&resp);
    assert!(kinds.iter().any(|k| k == "hardcoded_variable"), "got {kinds:?}");
}

#[test]
fn lint_interact_rejects_denied_external() {
    let mut host = Host::spawn();
    let resp = host.interact(serde_json::json!({
        "args_schema": {"noop": "int"},
        "result_schema": {"out": "int"},
        "args": {"noop": 0},
        "body": "{ x: (^awk 'x' | str trim), out: 0 }",
    }));
    assert_eq!(envelope_error_kind(&resp), Some("lint::violations"), "got {resp}");
    let kinds = lint_violation_kinds(&resp);
    assert!(kinds.iter().any(|k| k == "denied_command"), "got {kinds:?}");
}

// ----------------------------------------------------------------------------
// Slice 5.2: define_function body lint
// ----------------------------------------------------------------------------

#[test]
fn lint_define_function_rejects_hardcoded_path() {
    let mut host = Host::spawn();
    let mirror = host.source_dir("mirror");
    std::fs::create_dir_all(&mirror).expect("mkdir mirror");
    let reg = host.register("lib1", mirror.to_str().unwrap());
    assert!(!has_envelope_error(&reg), "register failed: {reg}");
    let resp = host.define_function(serde_json::json!({
        "library": "lib1",
        "module_path": "",
        "name": "bad",
        "args_schema": {"noop": "int"},
        "result_schema": {"out": "int"},
        "body": "{ p: \"/home/box/x\", out: 0 }"
    }));
    assert_eq!(envelope_error_kind(&resp), Some("lint::violations"), "got {resp}");
    let kinds = lint_violation_kinds(&resp);
    assert!(kinds.iter().any(|k| k == "hardcoded_variable"), "got {kinds:?}");
}

#[test]
fn lint_define_function_rejects_denied_external() {
    let mut host = Host::spawn();
    let mirror = host.source_dir("mirror2");
    std::fs::create_dir_all(&mirror).expect("mkdir mirror");
    let reg = host.register("lib2", mirror.to_str().unwrap());
    assert!(!has_envelope_error(&reg), "register failed: {reg}");
    let resp = host.define_function(serde_json::json!({
        "library": "lib2",
        "module_path": "",
        "name": "bad",
        "args_schema": {"noop": "int"},
        "result_schema": {"out": "int"},
        "body": "{ x: (^rm -rf /; 0) }"
    }));
    assert_eq!(envelope_error_kind(&resp), Some("lint::violations"), "got {resp}");
    let kinds = lint_violation_kinds(&resp);
    assert!(kinds.iter().any(|k| k == "denied_command"), "got {kinds:?}");
}

#[test]
fn lint_define_function_passes_clean_body() {
    let mut host = Host::spawn();
    let mirror = host.source_dir("mirror3");
    std::fs::create_dir_all(&mirror).expect("mkdir mirror");
    let reg = host.register("lib3", mirror.to_str().unwrap());
    assert!(reg.get("error").is_none(), "register failed: {reg}");
    let resp = host.define_function(serde_json::json!({
        "library": "lib3",
        "module_path": "",
        "name": "good",
        "args_schema": {"x": "int"},
        "result_schema": {"out": "int"},
        "body": "{ out: ($args.x + 1) }"
    }));
    // Expect success: no envelope error, no structuredContent on the
    // result (no-return tools omit structuredContent on success per the
    // C6.1 / 0.0.34 design).
    assert!(!has_envelope_error(&resp), "define rejected: {resp}");
    let result = resp.get("result").unwrap_or_else(|| {
        panic!("expected ok result; got {resp}");
    });
    assert!(
        result.get("structuredContent").is_none(),
        "no-return tools should not emit structuredContent; got {result}",
    );
}
