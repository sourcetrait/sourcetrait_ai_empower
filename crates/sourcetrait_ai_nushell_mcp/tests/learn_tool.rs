//! learn() tool surface test.
//!
//! Verifies learn(harness_dir) renders the embedded /nu skill template
//! and writes <harness_dir>/skills/nu/SKILL.md:
//! - envelope carries written_path / bytes / version (== CARGO_PKG_VERSION).
//! - the file exists at the composed path with bytes > 0 matching the body.
//! - the body is rendered (no literal `{{ version }}`; the stamp shows the
//!   live version) and retains the skill frontmatter.

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

    fn initialize(&mut self) {
        let id = self.next_id();
        let init = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "learn_tool", "version": "0.0.1"}
            }
        });
        self.send(&init);
        let _ = self.read_id(id);
        self.send(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }));
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
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn learn_writes_versioned_skill() {
    let mut host = Host::spawn();
    let harness = tempfile::tempdir().expect("harness tempdir");

    let resp = host.call_tool(
        "learn",
        serde_json::json!({ "harness_dir": harness.path().to_str().unwrap() }),
    );
    let env = resp["result"]
        .get("structuredContent")
        .unwrap_or_else(|| panic!("expected structuredContent; got {resp}"));
    assert!(env.get("error").is_none(), "learn returned an error: {env}");

    let version = env["version"].as_str().expect("version field");
    assert_eq!(
        version,
        env!("CARGO_PKG_VERSION"),
        "stamped version should match the crate version",
    );

    let bytes = env["bytes"].as_u64().expect("bytes field");
    assert!(bytes > 0, "written skill should be non-empty");

    let written = env["written_path"].as_str().expect("written_path field");
    let expected = harness.path().join("skills").join("nu").join("SKILL.md");
    assert_eq!(
        written,
        expected.to_str().unwrap(),
        "written_path should be <harness>/skills/nu/SKILL.md",
    );

    let body = std::fs::read_to_string(&expected).expect("read written skill");
    assert_eq!(
        body.len() as u64,
        bytes,
        "reported bytes should match the written file length",
    );
    assert!(
        !body.contains("{{ version }}"),
        "template should be rendered, not raw liquid",
    );
    assert!(
        body.contains(&format!("server v{version}")),
        "stamp line should carry the live version v{version}",
    );
    assert!(
        body.contains("name: nu"),
        "generated skill should retain its frontmatter",
    );
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
