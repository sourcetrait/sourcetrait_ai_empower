//! Phase 4 smoke tests for the MTP.
//!
//! Each test spawns its own host process so test isolation is per-test.
//! The closure `exit` test, in particular, would otherwise wreck other
//! tests sharing the worker.

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
                "clientInfo": {"name": "smoke", "version": "0.0.1"}
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

    fn run(&mut self, args: serde_json::Value) -> serde_json::Value {
        let id = self.next_id();
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": "run", "arguments": args}
        });
        self.send(&req);
        self.read_id(id)
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

fn extract_envelope(call_response: &serde_json::Value) -> Option<serde_json::Value> {
    let result = call_response.get("result")?;
    let content = result.get("content")?.as_array()?;
    let text = content.first()?.get("text")?.as_str()?;
    serde_json::from_str(text).ok()
}

#[test]
fn smoke_2_runtime_arg_typecheck_error() {
    // The substitution template puts ARGS_DATA as a literal record at the
    // __exec call site, so the arg typecheck fires at parse time inside the
    // worker. The wire returns ok=false with a structured parse-error
    // diagnostic, which the host maps to an MCP error.
    let mut host = Host::spawn();
    let args = serde_json::json!({
        "args_schema": "x: int",
        "result_schema": "out: int",
        "args": {"x": "five"},
        "body": "{ out: ($args.x + 1) }",
    });
    let resp = host.run(args);
    // We expect either an error result (rmcp's CallToolResult with is_error=true)
    // OR a JSON-RPC error object. Both are valid representations.
    let has_error_path = resp.get("error").is_some()
        || resp
            .get("result")
            .and_then(|r| r.get("isError"))
            .and_then(|v| v.as_bool())
            == Some(true);
    assert!(
        has_error_path,
        "expected parse-time arg mismatch to surface as error; got {resp}",
    );
}

#[test]
fn smoke_3_runtime_result_typecheck_error() {
    // Closure returns {out: "five"} but result_schema declares out: int. The
    // __resolve typed positional check fires at runtime and the worker emits
    // ok=false with the cant_convert error.
    let mut host = Host::spawn();
    let args = serde_json::json!({
        "args_schema": "x: int",
        "result_schema": "out: int",
        "args": {"x": 5},
        "body": "{ out: \"five\" }",
    });
    let resp = host.run(args);
    let has_error_path = resp.get("error").is_some()
        || resp
            .get("result")
            .and_then(|r| r.get("isError"))
            .and_then(|v| v.as_bool())
            == Some(true);
    assert!(
        has_error_path,
        "expected runtime result mismatch to surface as error; got {resp}",
    );
}

#[test]
fn smoke_5_external_command() {
    // External `^printf "hello"` should round-trip the stdout. The closure
    // captures the output as a string and emits {out: <captured>}.
    // Uses `printf` (POSIX, universal, NOT in slice 5.1 lint denylist)
    // because `^echo` is denied by the body linter.
    let mut host = Host::spawn();
    let args = serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: string",
        "args": {"noop": 0},
        "body": "{ out: (^printf hello | str trim) }",
    });
    let resp = host.run(args);
    let envelope = extract_envelope(&resp)
        .unwrap_or_else(|| panic!("expected envelope; got {resp}"));
    assert_eq!(
        envelope["result"]["out"].as_str(),
        Some("hello"),
        "expected {{out: \"hello\"}}; got {:?}",
        envelope["result"],
    );
}

#[test]
fn smoke_6_worker_death_via_exit() {
    // `exit 1` inside the closure body calls process::exit at the top frame
    // of eval_block, killing the worker. The host's send_request sees an EOF
    // on the IPC channel and returns an error. Subsequent calls should also
    // fail because the worker is gone (no respawn in MTP).
    let mut host = Host::spawn();
    let args = serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "args": {"noop": 0},
        "body": "{ out: (exit 1; 0) }",
    });
    let resp = host.run(args);
    let has_error_path = resp.get("error").is_some()
        || resp
            .get("result")
            .and_then(|r| r.get("isError"))
            .and_then(|v| v.as_bool())
            == Some(true);
    assert!(
        has_error_path,
        "expected worker exit to surface as error; got {resp}",
    );
}

#[test]
fn smoke_9_timeout_fires() {
    // Slice 5.10: timeout_ms wraps the round-trip in tokio::time::timeout.
    // A closure that sleeps longer than the timeout should return code
    // -32001 with a "timeout:" message; the worker is killed and the
    // next call succeeds on a fresh pool worker.
    let mut host = Host::spawn();
    let args = serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "args": {"noop": 0},
        // Multi-statement body without outer braces; inserted by the
        // template as the def body. `sleep 5sec` blocks the worker for
        // 5 seconds; the 200ms timeout fires first.
        "body": "sleep 5sec\n{ out: 0 }",
        "functions": [],
        "timeout_ms": 200u64
    });
    let resp = host.run(args);
    let err = resp.get("error").unwrap_or_else(|| {
        panic!("expected error envelope; got {resp}");
    });
    assert_eq!(err["code"].as_i64(), Some(-32001), "got {err}");
    assert!(
        err["message"].as_str().unwrap_or("").contains("timeout"),
        "expected 'timeout' in message; got {err}",
    );
    // Next call against the (respawned) pool worker should succeed.
    let args2 = serde_json::json!({
        "args_schema": "x: int",
        "result_schema": "out: int",
        "args": {"x": 7},
        "body": "{ out: ($args.x + 1) }",
    });
    let resp2 = host.run(args2);
    let env = extract_envelope(&resp2)
        .unwrap_or_else(|| panic!("expected envelope; got {resp2}"));
    assert_eq!(env["result"]["out"].as_i64(), Some(8), "got {env}");
}

#[test]
fn smoke_10_processes_empty_when_idle() {
    // Slice 5.9: processes() returns the in-flight list. When nothing
    // is running, the list is empty.
    let mut host = Host::spawn();
    let resp = host.call_tool("processes", serde_json::json!({}));
    let result = resp.get("result").unwrap_or_else(|| {
        panic!("expected ok result; got {resp}");
    });
    let text = result
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or_else(|| panic!("expected text; got {resp}"));
    let env: serde_json::Value = serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("parse: {e}"));
    let list = env["processes"]
        .as_array()
        .unwrap_or_else(|| panic!("expected processes array; got {env}"));
    assert!(list.is_empty(), "expected empty in-flight; got {list:?}");
}

#[test]
fn smoke_11_kill_unknown_nonce_silent_ok() {
    // Slice 5.9: kill() with an unknown nonce returns {ok: true} silently.
    let mut host = Host::spawn();
    let resp = host.call_tool("kill", serde_json::json!({"nonce": "doesnotexist"}));
    let result = resp.get("result").unwrap_or_else(|| {
        panic!("expected ok result; got {resp}");
    });
    let text = result
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or_else(|| panic!("expected text; got {resp}"));
    assert!(text.contains("\"ok\":true"), "got {text}");
}

#[test]
fn smoke_8_plugin_path_resolves() {
    // Slice 5.7: the worker's WarmBase::new sets engine_state.plugin_path
    // to <nu_config_dir>/plugin.msgpackz so that `$nu.plugin-path` returns
    // a string instead of `nothing`. Tests that this is visible from inside
    // a closure -- proves the load_plugins_best_effort path ran without
    // crashing AND the field assignment took effect.
    let mut host = Host::spawn();
    let args = serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "path: string",
        "args": {"noop": 0},
        "body": "{ path: $nu.plugin-path }",
    });
    let resp = host.run(args);
    let envelope = extract_envelope(&resp)
        .unwrap_or_else(|| panic!("expected envelope; got {resp}"));
    let path = envelope["result"]["path"]
        .as_str()
        .unwrap_or_else(|| panic!("expected string; got {:?}", envelope["result"]));
    // The resolved path should end with plugin.msgpackz (modulo platform
    // path separators). Non-empty and ends with the canonical filename.
    assert!(
        path.ends_with("plugin.msgpackz"),
        "expected $nu.plugin-path to end with plugin.msgpackz; got {path:?}",
    );
}

#[test]
fn smoke_7_multi_call_stability_and_scoping() {
    // Ten distinct closures in sequence on the same worker. Each call's def
    // for __exec lives only inside the do block, so the worker's EngineState
    // should not accumulate defs across calls. We verify by introspecting
    // `scope commands` AFTER the 10 calls -- the result should NOT contain
    // a definition named __exec (or any prior __exec lingering).
    let mut host = Host::spawn();
    for i in 0..10 {
        let args = serde_json::json!({
            "args_schema": "x: int",
            "result_schema": "out: int",
            "args": {"x": i as i64},
            "body": "{ out: ($args.x + 100) }",
        });
        let resp = host.run(args);
        let envelope = extract_envelope(&resp).unwrap_or_else(|| {
            panic!("call {i}: expected envelope; got {resp}")
        });
        let expected = (i + 100) as i64;
        assert_eq!(
            envelope["result"]["out"].as_i64(),
            Some(expected),
            "call {i}: expected {{out: {expected}}}; got {:?}",
            envelope["result"],
        );
    }
    // Introspect: ask the worker whether __exec exists at the top level after
    // all 10 calls. The do-block scoping should mean __exec does NOT persist.
    let intro = serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "leaked: int",
        "args": {"noop": 0},
        "body": "{ leaked: (scope commands | where name == \"__exec\" | length) }",
    });
    let resp = host.run(intro);
    let envelope = extract_envelope(&resp)
        .unwrap_or_else(|| panic!("intro: expected envelope; got {resp}"));
    assert_eq!(
        envelope["result"]["leaked"].as_i64(),
        Some(0),
        "do-block scoping should keep __exec out of the persistent \
         EngineState; got {:?}",
        envelope["result"],
    );
}
