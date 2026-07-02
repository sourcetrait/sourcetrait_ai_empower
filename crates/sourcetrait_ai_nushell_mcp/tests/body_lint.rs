//! Integration tests for the AST body linter on run() / interact().
//! Each handler that accepts agent-authored body source short-circuits
//! on lint violations with the agent-fixable `lint::<class> [L:C]`
//! report shape. `rerun()` does NOT re-lint per the_user 2026-05-31
//! design call (trust the cache). Library commit() does NOT lint
//! authored bodies, so no define-time body-lint coverage lives here.

use std::io::{BufRead, BufReader, Write};
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
}

impl Host {
    fn spawn() -> Self {
        let host_bin = env!("CARGO_BIN_EXE_nushell_mcp");
        let worker_bin = env!("CARGO_BIN_EXE_nushell_mcp_worker");
        let data_dir = tempfile::tempdir().expect("data tempdir");
        let cache_dir = tempfile::tempdir().expect("cache tempdir");
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
        };
        host.initialize();
        host
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
            "params": {"name": tool, "arguments": _author_prefixed(tool, args)}
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

/// The diagnostic kinds in the unified envelope's `errors` bucket.
fn lint_violation_kinds(resp: &serde_json::Value) -> Vec<String> {
    envelope_error(resp)
        .and_then(|e| e.get("errors"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.get("kind").and_then(|k| k.as_str()).map(str::to_string))
                .collect()
        })
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
    let kinds = lint_violation_kinds(&resp);
    assert!(
        kinds.iter().any(|k| k == "lint::hardcoded_variable"),
        "expected hardcoded_variable in violations; got {kinds:?}"
    );
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
    let kinds = lint_violation_kinds(&resp);
    assert!(
        kinds.iter().any(|k| k == "lint::denied_command"),
        "expected denied_command in violations; got {kinds:?}"
    );
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
    let kinds = lint_violation_kinds(&resp);
    assert!(kinds.iter().any(|k| k == "lint::denied_command"), "got {kinds:?}");
    assert!(
        kinds.iter().any(|k| k == "lint::hardcoded_variable"),
        "got {kinds:?}"
    );
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
    let kinds = lint_violation_kinds(&resp);
    assert!(
        kinds.iter().any(|k| k == "lint::hardcoded_variable"),
        "got {kinds:?}"
    );
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
    let kinds = lint_violation_kinds(&resp);
    assert!(kinds.iter().any(|k| k == "lint::denied_command"), "got {kinds:?}");
}

#[allow(dead_code)]
fn _author_prefixed(tool: &str, mut args: serde_json::Value) -> serde_json::Value {
    // Compound-library convention: default-author bare names at the dispatch boundary.
    fn pfx_lib(s: &str) -> String {
        if s.is_empty() || s.contains("/") {
            s.to_string()
        } else {
            format!("sourcetrait/{s}")
        }
    }
    fn pfx_np(s: &str) -> String {
        let lib = s.split(":").next().unwrap_or(s);
        if lib.is_empty() || lib.contains("/") {
            s.to_string()
        } else {
            format!("sourcetrait/{s}")
        }
    }
    match tool {
        "call" | "inspect" => {
            if let Some(np) = args.get("namepath").and_then(|v| v.as_str()) {
                let p = pfx_np(np);
                args["namepath"] = serde_json::Value::String(p);
            }
        }
        "new" => {
            if let Some(arr) = args.get_mut("namepaths").and_then(|v| v.as_array_mut()) {
                for e in arr.iter_mut() {
                    if let Some(s) = e.as_str() {
                        let p = pfx_np(s);
                        *e = serde_json::Value::String(p);
                    }
                }
            }
        }
        "library" | "commit" => {
            if let Some(l) = args.get("library").and_then(|v| v.as_str()) {
                let p = pfx_lib(l);
                args["library"] = serde_json::Value::String(p);
            }
        }
        _ => {}
    }
    args
}
