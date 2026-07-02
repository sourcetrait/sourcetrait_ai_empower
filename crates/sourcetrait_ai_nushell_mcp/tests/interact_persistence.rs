//! Cross-call persistence contract for `interact()`.
//!
//! Verifies the claims interact() makes against its stateful worker
//! substrate:
//!   1. In-body `$env.X = ...` mutations persist into the next call.
//!   2. In-body `cd <path>` propagates `$env.PWD` into the next call.
//!   3. Interact state does NOT leak into `run()` (which routes through
//!      a separate stateless pool worker).
//!
//! Mechanism: `build_interact_source` wraps the agent body in a
//! `def --env __interact [args: A]: nothing -> R { BODY }` invoked
//! inside a `( ... )` subexpression. `def --env` carries the body's
//! `$env` + `cd` out to the caller; `()` (not `do {}`) lets them reach
//! eval-top, where the worker's Stateful branch calls
//! `merge_env(&mut stack)` so they flow into `engine_state` for the
//! next call. Agent defs in the body are LOCAL to `__interact` and do
//! NOT persist -- the old top-level-body form persisted them as an
//! accidental byproduct, never a contract, so that case is retired.

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
                "clientInfo": {"name": "interact_persistence", "version": "0.0.1"}
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
            "params": {"name": tool, "arguments": _author_prefixed(tool, args)}
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

fn extract_envelope(call_response: &serde_json::Value) -> Option<serde_json::Value> {
    let result = call_response.get("result")?;
    if let Some(sc) = result.get("structuredContent") {
        return Some(sc.clone());
    }
    let content = result.get("content")?.as_array()?;
    let text = content.first()?.get("text")?.as_str()?;
    serde_json::from_str(text).ok()
}

#[test]
fn env_mutation_persists_across_interact_calls() {
    let mut host = Host::spawn();
    let first = host.call(
        "interact",
        serde_json::json!({
            "args_schema": {"value": "string"},
            "result_schema": {"wrote": "string"},
            "args": {"value": "alpha"},
            "body": "$env.SHOT_DEMO = $args.value\n{ wrote: $args.value }",
        }),
    );
    let env1 = extract_envelope(&first).unwrap_or_else(|| panic!("call 1 envelope; got {first}"));
    assert_eq!(env1["result"]["wrote"].as_str(), Some("alpha"));

    let second = host.call(
        "interact",
        serde_json::json!({
            "args_schema": {"noop": "int"},
            "result_schema": {"saw": "string"},
            "args": {"noop": 0},
            "body": "{ saw: $env.SHOT_DEMO }",
        }),
    );
    let env2 = extract_envelope(&second).unwrap_or_else(|| panic!("call 2 envelope; got {second}"));
    assert_eq!(
        env2["result"]["saw"].as_str(),
        Some("alpha"),
        "expected the env mutation from call 1 to persist; got {env2}",
    );
}

#[test]
fn cd_persists_across_interact_calls() {
    let mut host = Host::spawn();
    let _ = host.call(
        "interact",
        serde_json::json!({
            "args_schema": {"target": "string"},
            "result_schema": {"cwd": "string"},
            "args": {"target": "/tmp"},
            "body": "cd $args.target\n{ cwd: (pwd) }",
        }),
    );

    let second = host.call(
        "interact",
        serde_json::json!({
            "args_schema": {"noop": "int"},
            "result_schema": {"cwd": "string"},
            "args": {"noop": 0},
            "body": "{ cwd: $env.PWD }",
        }),
    );
    let env = extract_envelope(&second).unwrap_or_else(|| panic!("call 2 envelope; got {second}"));
    assert_eq!(
        env["result"]["cwd"].as_str(),
        Some("/tmp"),
        "expected cd from call 1 to propagate; got {env}",
    );
}

#[test]
fn interact_state_does_not_leak_into_run() {
    // Helper defined via interact() should NOT be visible to run()
    // which routes through the separate stateless pool worker.
    let mut host = Host::spawn();
    let _ = host.call(
        "interact",
        serde_json::json!({
            "args_schema": {"noop": "int"},
            "result_schema": {"ok": "bool"},
            "args": {"noop": 0},
            "body": "def leaked [] { 999 }\n{ ok: true }",
        }),
    );

    // Now invoke `leaked` via run(). Pool worker doesn't see it; the
    // worker eval returns an error.
    let resp = host.call(
        "run",
        serde_json::json!({
            "args_schema": {"noop": "int"},
            "result_schema": {"out": "int"},
            "args": {"noop": 0},
            "body": "{ out: (leaked) }",
        }),
    );
    let has_error_path = resp
        .get("result")
        .and_then(|r| r.get("structuredContent"))
        .and_then(|sc| sc.get("error"))
        .is_some();
    assert!(
        has_error_path,
        "expected run() to NOT see interact()'s `leaked` def; got {resp}",
    );
}

#[test]
fn multi_line_body_with_command_then_record_parses() {
    // Probe P7 (and the smoke_9 failure in C4) showed that subexpression-
    // wrapping multi-line bodies broke parsing. The new interact template
    // emits body at top level (no subexpression), so multi-line works the
    // same as a regular nu script.
    let mut host = Host::spawn();
    let resp = host.call(
        "interact",
        serde_json::json!({
            "args_schema": {"x": "int"},
            "result_schema": {"y": "int", "slept_ms": "int"},
            "args": {"x": 7},
            "body": "sleep 50ms\nlet doubled = ($args.x * 2)\n{ y: $doubled, slept_ms: 50 }",
        }),
    );
    let env = extract_envelope(&resp).unwrap_or_else(|| panic!("call envelope; got {resp}"));
    assert_eq!(env["result"]["y"].as_i64(), Some(14));
    assert_eq!(env["result"]["slept_ms"].as_i64(), Some(50));
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
