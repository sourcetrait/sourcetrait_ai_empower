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
}

impl Host {
    fn spawn() -> Self {
        let host_bin = env!("CARGO_BIN_EXE_nu_sh_mcp");
        let worker_bin = env!("CARGO_BIN_EXE_nu_sh_mcp_worker");
        let mut child = Command::new(host_bin)
            .env("NU_SH_MCP_WORKER_PATH", worker_bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn host");
        let stdin = child.stdin.take().expect("host stdin");
        let stdout = BufReader::new(child.stdout.take().expect("host stdout"));
        let mut host = Self { child, stdin, stdout, next_id: 1 };
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
        "closure_body": "{ out: ($args.x + 1) }",
        "functions": []
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
        "closure_body": "{ out: \"five\" }",
        "functions": []
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
fn smoke_4_function_helpers_in_scope() {
    // Helper closure registered in `functions` should be in scope inside the
    // do-block while __exec runs. The closure body invokes the helper.
    let mut host = Host::spawn();
    let args = serde_json::json!({
        "args_schema": "x: int",
        "result_schema": "out: int",
        "args": {"x": 5},
        "closure_body": "{ out: ((double {x: $args.x}).out + 1) }",
        "functions": [{
            "name": "double",
            "args_schema": "x: int",
            "result_schema": "out: int",
            "body": "{ out: ($args.x * 2) }"
        }]
    });
    let resp = host.run(args);
    let envelope = extract_envelope(&resp)
        .unwrap_or_else(|| panic!("expected envelope; got {resp}"));
    let result_str = envelope["result"]
        .as_str()
        .unwrap_or_else(|| panic!("result should be NUON string: {envelope}"));
    // double(5) -> {out: 10}; (10) + 1 -> 11; outer envelope -> {out: 11}
    assert!(
        result_str.contains("out") && result_str.contains("11"),
        "expected NUON record containing 'out' and '11'; got {result_str:?}",
    );
}

#[test]
fn smoke_5_external_command() {
    // External `^echo "hello"` should round-trip the stdout. The closure
    // captures the output as a string and emits {out: <captured>}.
    let mut host = Host::spawn();
    let args = serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: string",
        "args": {"noop": 0},
        "closure_body": "{ out: (^echo hello | str trim) }",
        "functions": []
    });
    let resp = host.run(args);
    let envelope = extract_envelope(&resp)
        .unwrap_or_else(|| panic!("expected envelope; got {resp}"));
    let result_str = envelope["result"]
        .as_str()
        .unwrap_or_else(|| panic!("result should be NUON string: {envelope}"));
    assert!(
        result_str.contains("hello"),
        "expected NUON record containing 'hello'; got {result_str:?}",
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
        "closure_body": "{ out: (exit 1; 0) }",
        "functions": []
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
            "closure_body": "{ out: ($args.x + 100) }",
            "functions": []
        });
        let resp = host.run(args);
        let envelope = extract_envelope(&resp).unwrap_or_else(|| {
            panic!("call {i}: expected envelope; got {resp}")
        });
        let result_str = envelope["result"].as_str().unwrap_or_else(|| {
            panic!("call {i}: result should be NUON string; got {envelope}")
        });
        let expected = (i + 100).to_string();
        assert!(
            result_str.contains(&expected),
            "call {i}: expected {expected} in result; got {result_str:?}",
        );
    }
    // Introspect: ask the worker whether __exec exists at the top level after
    // all 10 calls. The do-block scoping should mean __exec does NOT persist.
    let intro = serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "leaked: int",
        "args": {"noop": 0},
        "closure_body": "{ leaked: (scope commands | where name == \"__exec\" | length) }",
        "functions": []
    });
    let resp = host.run(intro);
    let envelope = extract_envelope(&resp)
        .unwrap_or_else(|| panic!("intro: expected envelope; got {resp}"));
    let result_str = envelope["result"]
        .as_str()
        .unwrap_or_else(|| panic!("intro: result NUON; got {envelope}"));
    assert!(
        result_str.contains("leaked") && result_str.contains("0"),
        "do-block scoping should keep __exec out of the persistent EngineState; \
         got {result_str:?}",
    );
}
