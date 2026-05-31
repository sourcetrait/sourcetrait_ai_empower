//! Slice 5.1 integration tests: run() handler short-circuits on lint
//! violations with the agent-fixable `lint::<class> [L:C]` report shape.
//! Helper-function lint + interact/define/import wire-ups land in slice
//! 5.2 + 5.3 -- not covered here.

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

fn error_text(resp: &serde_json::Value) -> Option<String> {
    // rmcp surfaces Err(ErrorData::invalid_params) as a JSON-RPC error
    // object at the top level: {"error": {"code": -32602, "message": ...}}.
    resp.get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .map(str::to_string)
}

#[test]
fn lint_rejects_closure_with_hardcoded_path() {
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "args": {"noop": 0},
        "closure": "{ p: \"/home/box/proj/x\", out: 0 }",
        "functions": []
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    assert!(
        msg.contains("lint::hardcoded_variable"),
        "expected lint::hardcoded_variable token; got {msg:?}",
    );
}

#[test]
fn lint_rejects_closure_with_blacklisted_external() {
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "args": {"noop": 0},
        "closure": "{ x: (^awk '{print $1}' | str trim), out: 0 }",
        "functions": []
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    assert!(
        msg.contains("lint::blacklisted_command"),
        "expected lint::blacklisted_command token; got {msg:?}",
    );
}

#[test]
fn lint_passes_clean_closure() {
    // No lint violations -> reaches the worker -> normal envelope path.
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": "x: int",
        "result_schema": "out: int",
        "args": {"x": 5},
        "closure": "{ out: ($args.x + 1) }",
        "functions": []
    }));
    // Either a normal result envelope or rmcp's CallToolResult shape;
    // both wrap a JSON text content carrying the worker envelope.
    let result = resp.get("result").unwrap_or_else(|| {
        panic!("expected ok result; got {resp}");
    });
    let content = result
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or_else(|| panic!("expected text content; got {resp}"));
    let env: serde_json::Value = serde_json::from_str(content)
        .unwrap_or_else(|e| panic!("parse envelope: {e}; got {content:?}"));
    assert_eq!(env["result"]["out"].as_i64(), Some(6), "got {env}");
}

#[test]
fn lint_aggregates_multiple_violations() {
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "args": {"noop": 0},
        "closure": "\
^awk 'x'
cd \"/a/b\"
{ out: 0 }",
        "functions": []
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    // Both violations appear in one message, newline-joined.
    assert!(msg.contains("lint::blacklisted_command"), "got {msg:?}");
    assert!(msg.contains("lint::hardcoded_variable"), "got {msg:?}");
}
