//! Caching + rerun() round-trip tests.
//!
//! Verifies:
//!   1. run() returns a deterministic, non-"0" rerun_id derived from
//!      (args_schema, result_schema, closure).
//!   2. Same closure shape returns the same rerun_id across calls.
//!   3. Different closure returns a different rerun_id.
//!   4. rerun() reconstructs the cached closure and evaluates with
//!      the supplied args.
//!   5. rerun() with an unknown rerun_id surfaces as an error.
//!   6. rerun() with a malformed rerun_id (non-base62) is rejected.
//!   7. interact() envelope has no rerun_id field.

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
                "clientInfo": {"name": "cache_rerun", "version": "0.0.1"}
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
    if let Some(sc) = result.get("structuredContent") {
        return Some(sc.clone());
    }
    let content = result.get("content")?.as_array()?;
    let text = content.first()?.get("text")?.as_str()?;
    serde_json::from_str(text).ok()
}

fn extract_rerun_id(call_response: &serde_json::Value) -> String {
    let env = extract_envelope(call_response)
        .unwrap_or_else(|| panic!("expected envelope; got {call_response}"));
    env["rerun_id"]
        .as_str()
        .unwrap_or_else(|| panic!("envelope missing rerun_id; got {env}"))
        .to_string()
}

#[test]
fn run_returns_deterministic_rerun_id() {
    let mut host = Host::spawn();
    let closure_a = serde_json::json!({
        "args_schema": "x: int",
        "result_schema": "out: int",
        "args": {"x": 1},
        "body": "{ out: ($args.x + 100) }",
    });
    let r1 = extract_rerun_id(&host.call("run", closure_a.clone()));
    let r2 = extract_rerun_id(&host.call("run", closure_a));
    assert_eq!(r1, r2, "same closure -> same rerun_id; got {r1} vs {r2}");
    assert_ne!(r1, "0", "rerun_id should be content-derived, not the placeholder \"0\"");
    assert!(
        !r1.is_empty() && r1.chars().all(|c| c.is_ascii_alphanumeric()),
        "rerun_id should be non-empty base62; got {r1:?}",
    );
}

#[test]
fn rerun_id_differs_when_closure_changes() {
    let mut host = Host::spawn();
    let base = serde_json::json!({
        "args_schema": "x: int",
        "result_schema": "out: int",
        "args": {"x": 1},
        "body": "{ out: ($args.x + 100) }",
    });
    let mut altered = base.clone();
    altered["body"] = serde_json::json!("{ out: ($args.x + 200) }");
    let r_base = extract_rerun_id(&host.call("run", base));
    let r_alt = extract_rerun_id(&host.call("run", altered));
    assert_ne!(r_base, r_alt, "different closure body -> different rerun_id");
}

#[test]
fn rerun_roundtrip_with_new_args() {
    let mut host = Host::spawn();
    let first = host.call(
        "run",
        serde_json::json!({
            "args_schema": "x: int",
            "result_schema": "out: int",
            "args": {"x": 5},
            "body": "{ out: ($args.x * 3) }",
        }),
    );
    let first_env = extract_envelope(&first)
        .unwrap_or_else(|| panic!("call 1 envelope; got {first}"));
    assert_eq!(first_env["result"]["out"].as_i64(), Some(15));
    let rerun_id = first_env["rerun_id"].as_str().expect("rerun_id present").to_string();

    let second = host.call(
        "rerun",
        serde_json::json!({
            "rerun_id": rerun_id,
            "args": {"x": 7}
        }),
    );
    let second_env = extract_envelope(&second)
        .unwrap_or_else(|| panic!("rerun envelope; got {second}"));
    assert_eq!(
        second_env["result"]["out"].as_i64(),
        Some(21),
        "rerun should reuse the cached closure with new args -> 7 * 3 = 21; got {:?}",
        second_env["result"],
    );
    // rerun envelope MUST NOT echo rerun_id (agent supplied it).
    assert!(
        second_env.get("rerun_id").is_none(),
        "rerun envelope should not include rerun_id; got {second_env}",
    );
}

#[test]
fn rerun_unknown_id_errors() {
    let mut host = Host::spawn();
    let resp = host.call(
        "rerun",
        serde_json::json!({
            "rerun_id": "abcDEF123456",
            "args": {"x": 0}
        }),
    );
    let has_error = resp.get("error").is_some()
        || resp
            .get("result")
            .and_then(|r| r.get("isError"))
            .and_then(|v| v.as_bool())
            == Some(true);
    assert!(has_error, "expected error for unknown rerun_id; got {resp}");
}

#[test]
fn rerun_rejects_non_base62_id() {
    let mut host = Host::spawn();
    let resp = host.call(
        "rerun",
        serde_json::json!({
            "rerun_id": "../etc/passwd",
            "args": {"x": 0}
        }),
    );
    let has_error = resp.get("error").is_some()
        || resp
            .get("result")
            .and_then(|r| r.get("isError"))
            .and_then(|v| v.as_bool())
            == Some(true);
    assert!(has_error, "expected error for non-base62 rerun_id; got {resp}");
}

#[test]
fn interact_envelope_has_no_rerun_id() {
    let mut host = Host::spawn();
    let resp = host.call(
        "interact",
        serde_json::json!({
            "args_schema": "x: int",
            "result_schema": "out: int",
            "args": {"x": 4},
            "body": "{ out: ($__args.x * 2) }",
        }),
    );
    let env = extract_envelope(&resp)
        .unwrap_or_else(|| panic!("interact envelope; got {resp}"));
    assert_eq!(env["result"]["out"].as_i64(), Some(8));
    assert!(
        env.get("rerun_id").is_none(),
        "interact envelope should not include rerun_id; got {env}",
    );
}
