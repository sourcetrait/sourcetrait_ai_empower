
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
    fn spawn_with(args: &[&str]) -> Self {
        let host_bin = env!("CARGO_BIN_EXE_nushell_mcp");
        let worker_bin = env!("CARGO_BIN_EXE_nushell_mcp_worker");
        let data_dir = tempfile::tempdir().expect("data tempdir");
        let cache_dir = tempfile::tempdir().expect("cache tempdir");
        let mut child = Command::new(host_bin)
            .args(args)
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
                "clientInfo": {"name": "deny", "version": "0.0.1"}
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

    fn list_tool_names(&mut self) -> Vec<String> {
        let id = self.next_id();
        self.send(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/list",
        }));
        let resp = self.read_id(id);
        resp["result"]["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .map(|t| t["name"].as_str().expect("tool name").to_string())
            .collect()
    }

    fn call(&mut self, tool: &str, args: serde_json::Value) -> serde_json::Value {
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

#[test]
fn deny_removes_tools_from_list() {
    let mut host = Host::spawn_with(&["--deny", "run,interact,learn"]);
    let names = host.list_tool_names();
    assert_eq!(names.len(), 9, "12 - 3 denied = 9; got {names:?}");
    for absent in ["run", "interact", "learn"] {
        assert!(
            !names.contains(&absent.to_string()),
            "denied `{absent}` must be absent; got {names:?}",
        );
    }
    for present in [
        "rerun", "call", "new", "commit", "library", "info", "inspect", "processes", "kill",
    ] {
        assert!(
            names.contains(&present.to_string()),
            "`{present}` should remain; got {names:?}",
        );
    }
}

#[test]
fn denied_tool_call_fails_at_protocol_layer() {
    let mut host = Host::spawn_with(&["--deny", "run"]);
    let resp = host.call(
        "run",
        serde_json::json!({
            "args_schema": {},
            "result_schema": {"out": "int"},
            "args": {},
            "body": "{ out: 1 }",
        }),
    );
    let success_envelope = resp
        .get("result")
        .and_then(|r| r.get("structuredContent"))
        .map(|sc| sc.get("error").is_none())
        .unwrap_or(false);
    assert!(
        !success_envelope,
        "a denied tool must not execute; got {resp}",
    );
}

#[test]
fn deny_full_set_leaves_core_four() {
    let mut host = Host::spawn_with(&[
        "--deny",
        "run,rerun,interact,call,learn,new,commit,library",
    ]);
    let mut names = host.list_tool_names();
    names.sort();
    assert_eq!(
        names,
        vec!["info", "inspect", "kill", "processes"],
        "the full deny set leaves exactly the core four",
    );
}

#[test]
fn unknown_deny_token_fails_startup() {
    let host_bin = env!("CARGO_BIN_EXE_nushell_mcp");
    let data_dir = tempfile::tempdir().expect("data tempdir");
    let cache_dir = tempfile::tempdir().expect("cache tempdir");
    let out = Command::new(host_bin)
        .args(["--deny", "bogus"])
        .env("XDG_DATA_HOME", data_dir.path())
        .env("XDG_CACHE_HOME", cache_dir.path())
        .output()
        .expect("run host");
    assert!(
        !out.status.success(),
        "unknown deny token should fail startup; got {:?}",
        out.status,
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("bogus"),
        "the clap error should name the bad token; got {stderr:?}",
    );
}
