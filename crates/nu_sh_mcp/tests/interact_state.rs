//! Interact() tool surface smoke.
//!
//! Full persistence coverage lives in `tests/interact_persistence.rs`
//! (env + cd across calls). This file only verifies the tool surface
//! advertised by `#[tool_router]`.

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

    #[allow(dead_code)]
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

#[allow(dead_code)]
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
    assert_eq!(names.len(), 12, "expected 12 tools; got {names:?}");
    for expected in [
        "run",
        "interact",
        "rerun",
        "register_library",
        "unregister_library",
        "define_function",
        "undefine_function",
        "import_library",
        "reimport_library",
        "call",
    ] {
        assert!(
            names.contains(&expected),
            "missing `{expected}` in {names:?}",
        );
    }
}

