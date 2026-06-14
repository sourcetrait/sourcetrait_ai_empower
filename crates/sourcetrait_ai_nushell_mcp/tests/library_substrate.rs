//! Library substrate tests for 0.0.10.
//!
//! Verifies:
//!   - First startup creates `<XDG_DATA_HOME>/sourcetrait/nushell_mcp/keypair/
//!     {id_nushell_mcp, id_nushell_mcp.pub, allowed_signers}` and the git repo at
//!     `<XDG_DATA_HOME>/sourcetrait/nushell_mcp/libraries/` with a signed initial commit.
//!   - `register_library(name, path)` writes the library subtree in the
//!     MCP repo + mirrors at the client `path`; commits.
//!   - Duplicate `register_library` errors.
//!   - `unregister_library` removes from the MCP repo; commits.
//!   - `unregister_library` on a missing name errors.
//!   - `tools/list` returns all 14 tools (membership-checked).

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
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
    client_mirror_root: tempfile::TempDir,
}

impl Host {
    fn spawn() -> Self {
        let host_bin = env!("CARGO_BIN_EXE_nushell_mcp");
        let worker_bin = env!("CARGO_BIN_EXE_nushell_mcp_worker");
        let data_dir = tempfile::tempdir().expect("data tempdir");
        let cache_dir = tempfile::tempdir().expect("cache tempdir");
        let client_mirror_root = tempfile::tempdir().expect("client mirror tempdir");
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
            client_mirror_root,
        };
        host.initialize();
        host
    }

    fn nushell_mcp_data_dir(&self) -> PathBuf {
        self.data_dir.path().join("sourcetrait").join("nushell_mcp")
    }

    fn libraries_dir(&self) -> PathBuf {
        self.nushell_mcp_data_dir().join("libraries")
    }

    fn keypair_dir(&self) -> PathBuf {
        self.nushell_mcp_data_dir().join("keypair")
    }

    fn library_dir(&self, name: &str) -> PathBuf {
        self.libraries_dir().join(name)
    }

    fn client_dir(&self, name: &str) -> PathBuf {
        self.client_mirror_root.path().join(name)
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
                "clientInfo": {"name": "library_substrate", "version": "0.0.1"}
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

    fn list_tools(&mut self) -> serde_json::Value {
        let id = self.next_id();
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/list",
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

fn has_error_path(resp: &serde_json::Value) -> bool {
    resp.get("result")
        .and_then(|r| r.get("structuredContent"))
        .and_then(|sc| sc.get("error"))
        .is_some()
}

#[test]
fn substrate_initializes_on_first_startup() {
    let host = Host::spawn();
    // Substrate files appear after first MCP message handshake (ensure_substrate
    // runs in run_server BEFORE serve(), so by initialize-response time these
    // exist).
    let keypair = host.keypair_dir();
    assert!(
        keypair.join("id_nushell_mcp").exists(),
        "private key should exist at {}",
        keypair.join("id_nushell_mcp").display(),
    );
    assert!(
        keypair.join("id_nushell_mcp.pub").exists(),
        "public key should exist",
    );
    assert!(
        keypair.join("allowed_signers").exists(),
        "allowed_signers should exist",
    );
    let libs = host.libraries_dir();
    assert!(libs.join(".git").exists(), ".git should exist in libraries");
    // Initial commit on main.
    let head = std::fs::read_to_string(libs.join(".git").join("HEAD")).expect("read HEAD");
    assert!(
        head.contains("refs/heads/main"),
        "HEAD should point at main; got {head:?}",
    );
}

#[test]
fn tools_list_has_fourteen() {
    let mut host = Host::spawn();
    let resp = host.list_tools();
    let tools = resp["result"]["tools"]
        .as_array()
        .expect("tools array");
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().expect("tool name"))
        .collect();
    assert_eq!(names.len(), 14, "expected 14 tools; got {names:?}");
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
        "learn",
    ] {
        assert!(
            names.contains(&expected),
            "missing `{expected}` in {names:?}",
        );
    }
}

#[test]
fn register_library_writes_repo_and_mirror() {
    let mut host = Host::spawn();
    let client_dir = host.client_dir("mylib");
    let resp = host.call(
        "register_library",
        serde_json::json!({
            "name": "mylib",
            "path": client_dir.to_str().expect("client_dir to str"),
        }),
    );
    assert!(
        !has_error_path(&resp),
        "register_library should succeed; got {resp}",
    );
    // MCP-side files exist.
    let lib_dir = host.library_dir("mylib");
    assert!(lib_dir.exists(), "lib dir should exist at {}", lib_dir.display());
    assert!(
        lib_dir.join("mod.nu").exists(),
        "lib mod.nu should exist",
    );
    let meta_path = lib_dir.join(".nushell_mcp_meta.json");
    assert!(meta_path.exists(), "meta sidecar should exist");
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&meta_path).expect("read meta"))
            .expect("decode meta");
    assert_eq!(meta["kind"].as_str(), Some("registered"));
    assert_eq!(
        meta["source_path"].as_str(),
        client_dir.to_str(),
    );
    // Client mirror exists.
    assert!(client_dir.exists(), "client mirror dir should exist");
    assert!(
        client_dir.join("mod.nu").exists(),
        "client mirror mod.nu should exist",
    );
    // Commit was made (HEAD shifted from initial empty commit).
    let log = git_log_subjects(&host.libraries_dir());
    assert!(
        log.iter().any(|s| s == "register library mylib"),
        "expected register commit in log; got {log:?}",
    );
}

#[test]
fn duplicate_register_errors() {
    let mut host = Host::spawn();
    let client_dir = host.client_dir("dup");
    let r1 = host.call(
        "register_library",
        serde_json::json!({
            "name": "dup",
            "path": client_dir.to_str().expect("client_dir to str"),
        }),
    );
    assert!(!has_error_path(&r1), "first register should succeed; got {r1}");
    let r2 = host.call(
        "register_library",
        serde_json::json!({
            "name": "dup",
            "path": client_dir.to_str().expect("client_dir to str"),
        }),
    );
    assert!(
        has_error_path(&r2),
        "duplicate register should error; got {r2}",
    );
}

#[test]
fn unregister_library_removes_subtree() {
    let mut host = Host::spawn();
    let client_dir = host.client_dir("droppable");
    let _ = host.call(
        "register_library",
        serde_json::json!({
            "name": "droppable",
            "path": client_dir.to_str().expect("client_dir to str"),
        }),
    );
    assert!(host.library_dir("droppable").exists());
    let resp = host.call(
        "unregister_library",
        serde_json::json!({"name": "droppable"}),
    );
    assert!(
        !has_error_path(&resp),
        "unregister_library should succeed; got {resp}",
    );
    assert!(
        !host.library_dir("droppable").exists(),
        "lib dir should be gone after unregister",
    );
    let log = git_log_subjects(&host.libraries_dir());
    assert!(
        log.iter().any(|s| s == "unregister library droppable"),
        "expected unregister commit in log; got {log:?}",
    );
}

#[test]
fn unregister_missing_errors() {
    let mut host = Host::spawn();
    let resp = host.call(
        "unregister_library",
        serde_json::json!({"name": "neverexisted"}),
    );
    assert!(
        has_error_path(&resp),
        "unregister of unknown lib should error; got {resp}",
    );
}

fn git_log_subjects(repo: &Path) -> Vec<String> {
    let out = Command::new("git")
        .arg("log")
        .arg("--pretty=%s")
        .current_dir(repo)
        .output()
        .expect("git log");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect()
}
