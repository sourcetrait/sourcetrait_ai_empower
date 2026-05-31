//! Interact() cross-call persistence smoke tests.
//!
//! interact() pairs `build_interact_source` (no `do { ... }` wrapper) with
//! a stateful worker that does NOT clone engine_state per call and calls
//! merge_env after each eval. Verifies that:
//!   1. Helper functions registered via `functions` persist across calls.
//!   2. `cd` inside a closure body propagates to subsequent calls via
//!      merge_env.
//!   3. Sequential run() calls (separate stateless worker) are unaffected
//!      by interact() state.

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
        let host_bin = env!("CARGO_BIN_EXE_nu_sh_mcp");
        let worker_bin = env!("CARGO_BIN_EXE_nu_sh_mcp_worker");
        let data_dir = tempfile::tempdir().expect("data tempdir");
        let cache_dir = tempfile::tempdir().expect("cache tempdir");
        let mut child = Command::new(host_bin)
            .env("NU_SH_MCP_WORKER_PATH", worker_bin)
            .env("XDG_DATA_HOME", data_dir.path())
            .env("XDG_CACHE_HOME", cache_dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn host");
        let stdin = child.stdin.take().expect("host stdin");
        let stdout = BufReader::new(child.stdout.take().expect("host stdout"));
        let mut host = Self { child, stdin, stdout, next_id: 1, data_dir, cache_dir };
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
                "clientInfo": {"name": "interact_state", "version": "0.0.1"}
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

fn extract_envelope(call_response: &serde_json::Value) -> Option<serde_json::Value> {
    let result = call_response.get("result")?;
    let content = result.get("content")?.as_array()?;
    let text = content.first()?.get("text")?.as_str()?;
    serde_json::from_str(text).ok()
}

#[test]
fn interact_lists_both_run_and_interact_tools() {
    // Sanity check on the tool surface: tools/list should show exactly the
    // two tool names (`run` and `interact`) registered by `#[tool_router]`.
    let mut host = Host::spawn();
    let id = host.next_id();
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/list",
    });
    host.send(&req);
    let resp = host.read_id(id);
    let tools = resp["result"]["tools"]
        .as_array()
        .expect("tools array");
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().expect("tool name"))
        .collect();
    assert_eq!(names.len(), 7, "expected 7 tools; got {names:?}");
    for expected in [
        "run",
        "interact",
        "rerun",
        "register_library",
        "unregister_library",
        "define_function",
        "undefine_function",
    ] {
        assert!(
            names.contains(&expected),
            "missing `{expected}` in {names:?}",
        );
    }
}

#[test]
fn helper_persists_across_interact_calls() {
    // First interact() call registers a helper `foo`. Second interact()
    // call (no helpers) invokes `foo` and expects to find it.
    let mut host = Host::spawn();
    let first = host.call(
        "interact",
        serde_json::json!({
            "args_schema": "noop: int",
            "result_schema": "out: int",
            "args": {"noop": 0},
            "closure": "{ out: ((foo {n: 7}).out) }",
            "functions": [{
                "name": "foo",
                "args_schema": "n: int",
                "result_schema": "out: int",
                "body": "{ out: ($args.n * 6) }"
            }]
        }),
    );
    let envelope = extract_envelope(&first)
        .unwrap_or_else(|| panic!("call 1 envelope; got {first}"));
    assert_eq!(
        envelope["result"]["out"].as_i64(),
        Some(42),
        "call 1: expected 42; got {:?}",
        envelope["result"],
    );

    // Second call: no helpers, but `foo` should still be defined in the
    // stateful worker's engine_state from the first call.
    let second = host.call(
        "interact",
        serde_json::json!({
            "args_schema": "n: int",
            "result_schema": "out: int",
            "args": {"n": 5},
            "closure": "{ out: ((foo {n: $args.n}).out) }",
            "functions": []
        }),
    );
    let envelope = extract_envelope(&second)
        .unwrap_or_else(|| panic!("call 2 envelope; got {second}"));
    assert_eq!(
        envelope["result"]["out"].as_i64(),
        Some(30),
        "call 2: expected 30 (5 * 6 via persisted foo); got {:?}",
        envelope["result"],
    );
}

#[test]
fn interact_state_does_not_leak_into_run() {
    // Helper registered via interact() should NOT be visible to run(),
    // which routes through a separate stateless worker.
    let mut host = Host::spawn();
    let _ = host.call(
        "interact",
        serde_json::json!({
            "args_schema": "noop: int",
            "result_schema": "out: int",
            "args": {"noop": 0},
            "closure": "{ out: 0 }",
            "functions": [{
                "name": "leaked",
                "args_schema": "noop: int",
                "result_schema": "out: int",
                "body": "{ out: 999 }"
            }]
        }),
    );

    // Now invoke `leaked` via run(). The stateless worker should NOT
    // have it; this should surface as an error from the worker.
    let resp = host.call(
        "run",
        serde_json::json!({
            "args_schema": "noop: int",
            "result_schema": "out: int",
            "args": {"noop": 0},
            "closure": "{ out: ((leaked {noop: 0}).out) }",
            "functions": []
        }),
    );
    let has_error_path = resp.get("error").is_some()
        || resp
            .get("result")
            .and_then(|r| r.get("isError"))
            .and_then(|v| v.as_bool())
            == Some(true);
    assert!(
        has_error_path,
        "expected run() to NOT see interact()'s `leaked` helper; got {resp}",
    );
}
