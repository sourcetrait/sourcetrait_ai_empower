//! Worker command-context layer tests (item 17).
//!
//! run() (stateless) loads lang + shell + nu-cmd-extra; interact()
//! (stateful) additionally loads nu-cmd-plugin. So `bits` / `str *-case`
//! resolve on both workers, but the `plugin *` admin family resolves
//! ONLY on interact() -- run() stays admin-free.

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
        self.send(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "command_context", "version": "0.0.1"}
            }
        }));
        let _ = self.read_id(id);
        self.send(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }));
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
        self.send(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": tool, "arguments": args}
        }));
        self.read_id(id)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn envelope(resp: &serde_json::Value) -> serde_json::Value {
    resp.get("result")
        .and_then(|r| r.get("structuredContent"))
        .cloned()
        .unwrap_or_else(|| panic!("expected structuredContent; got {resp}"))
}

#[test]
fn run_has_extra_lacks_plugin() {
    // run() = lang + shell + extra. `bits and` + `str snake-case` resolve
    // and compute; the `plugin *` admin family is absent.
    let mut host = Host::spawn();
    let resp = host.call_tool(
        "run",
        serde_json::json!({
            "args_schema": {},
            "result_schema": {"bits": "bool", "snake": "bool", "plugin": "bool", "bits_and": "int"},
            "args": {},
            "body": "let names = (scope commands | get name)\n{ bits: (\"bits and\" in $names), snake: (\"str snake-case\" in $names), plugin: (\"plugin list\" in $names), bits_and: (5 | bits and 3) }",
        }),
    );
    let env = envelope(&resp);
    assert_eq!(
        env["result"]["bits"].as_bool(),
        Some(true),
        "run() should have `bits and` (nu-cmd-extra); got {env}",
    );
    assert_eq!(
        env["result"]["snake"].as_bool(),
        Some(true),
        "run() should have `str snake-case` (nu-cmd-extra); got {env}",
    );
    assert_eq!(
        env["result"]["plugin"].as_bool(),
        Some(false),
        "run() must NOT have the `plugin *` admin family; got {env}",
    );
    assert_eq!(
        env["result"]["bits_and"].as_i64(),
        Some(1),
        "extra `bits and` should compute 5 & 3 = 1; got {env}",
    );
}

#[test]
fn interact_has_extra_and_plugin() {
    // interact() = lang + shell + extra + plugin. Both `bits and` and the
    // `plugin *` admin family resolve on the stateful administrative worker.
    let mut host = Host::spawn();
    let resp = host.call_tool(
        "interact",
        serde_json::json!({
            "args_schema": {},
            "result_schema": {"bits": "bool", "plugin": "bool"},
            "args": {},
            "body": "let names = (scope commands | get name)\n{ bits: (\"bits and\" in $names), plugin: (\"plugin list\" in $names) }",
        }),
    );
    let env = envelope(&resp);
    assert_eq!(
        env["result"]["bits"].as_bool(),
        Some(true),
        "interact() should have `bits and` (nu-cmd-extra); got {env}",
    );
    assert_eq!(
        env["result"]["plugin"].as_bool(),
        Some(true),
        "interact() (admin) should have the `plugin *` family (nu-cmd-plugin); got {env}",
    );
}
