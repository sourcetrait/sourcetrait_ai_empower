//! Library substrate tests.
//!
//! Verifies:
//!   - First startup creates `<XDG_DATA_HOME>/sourcetrait/nushell_mcp/keypair/
//!     {id_nushell_mcp, id_nushell_mcp.pub, allowed_signers}` and the git repo at
//!     `<XDG_DATA_HOME>/sourcetrait/nushell_mcp/libraries/` with a signed initial commit.
//!   - `new(name, source_path)` writes the library subtree in the MCP repo
//!     and lands a signed git commit in the repo log.
//!   - `delete(name, source_path)` removes the subtree from the MCP repo and
//!     lands a signed git commit in the repo log.
//!   - `delete` on a missing name errors.
//!   - `tools/list` returns all 11 tools (membership-checked).

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
    source_root: tempfile::TempDir,
}

impl Host {
    fn spawn() -> Self {
        let host_bin = env!("CARGO_BIN_EXE_nushell_mcp");
        let worker_bin = env!("CARGO_BIN_EXE_nushell_mcp_worker");
        let data_dir = tempfile::tempdir().expect("data tempdir");
        let cache_dir = tempfile::tempdir().expect("cache tempdir");
        let source_root = tempfile::tempdir().expect("source tempdir");
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
            source_root,
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

    fn source_dir(&self, name: &str) -> PathBuf {
        self.source_root.path().join(name)
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

fn write_source(dir: &Path, rel: &str, contents: &str) {
    let target = dir.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(&target, contents).expect("write source");
}

fn valid_function_source(args_schema: &str, result_schema: &str, body: &str) -> String {
    // the 1-def `main` contract: main owns the body; the result schema comes
    // from the `: nothing -> R` output type.
    format!(
        "export def main [args: record<{args_schema}>]: nothing -> record<{result_schema}> {{\n{body}\n}}\n",
    )
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
fn tools_list_has_eleven() {
    let mut host = Host::spawn();
    let resp = host.list_tools();
    let tools = resp["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().expect("tool name"))
        .collect();
    assert_eq!(names.len(), 12, "expected 12 tools; got {names:?}");
    for expected in [
        "run",
        "interact",
        "rerun",
        "call",
        "processes",
        "kill",
        "info",
        "learn",
        "new",
        "commit",
        "delete",
        "inspect",
    ] {
        assert!(
            names.contains(&expected),
            "missing `{expected}` in {names:?}",
        );
    }
}

#[test]
fn new_writes_repo_and_records_meta() {
    let mut host = Host::spawn();
    let src = host.source_dir("mylib");
    let resp = host.call(
        "new",
        serde_json::json!({
            "library": "mylib",
            "source_path": src.to_str().expect("src to str"),
        }),
    );
    assert!(!has_error_path(&resp), "new should succeed; got {resp}");
    // MCP-side files exist.
    let lib_dir = host.library_dir("mylib");
    assert!(
        lib_dir.exists(),
        "lib dir should exist at {}",
        lib_dir.display()
    );
    assert!(lib_dir.join("mod.nu").exists(), "lib mod.nu should exist",);
    let meta_path = lib_dir.join(".meta/library.json");
    assert!(meta_path.exists(), "meta sidecar should exist");
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&meta_path).expect("read meta"))
            .expect("decode meta");
    // The 0.0.44 meta holds ONLY source_path (no kind discriminant).
    assert_eq!(meta["source_path"].as_str(), src.to_str(),);
    assert!(
        meta.get("kind").is_none(),
        "meta should not carry a kind field; got {meta}",
    );
    // Agent source tree seeded with its root mod.nu.
    assert!(src.exists(), "source dir should exist");
    assert!(
        src.join("mod.nu").exists(),
        "source root mod.nu should be seeded",
    );
    // A signed commit landed in the repo log.
    let log = git_log_subjects(&host.libraries_dir());
    assert!(
        log.iter().any(|s| s == "new library mylib"),
        "expected new-library commit in log; got {log:?}",
    );
}

#[test]
fn new_reestablish_with_source_path_errors() {
    let mut host = Host::spawn();
    let src = host.source_dir("dup");
    let r1 = host.call(
        "new",
        serde_json::json!({
            "library": "dup",
            "source_path": src.to_str().expect("src to str"),
        }),
    );
    assert!(!has_error_path(&r1), "first new should succeed; got {r1}");
    // Re-passing source_path on an already-established library is rejected.
    let r2 = host.call(
        "new",
        serde_json::json!({
            "library": "dup",
            "source_path": src.to_str().expect("src to str"),
        }),
    );
    assert!(
        has_error_path(&r2),
        "re-establishing with source_path should error; got {r2}",
    );
}

#[test]
fn delete_removes_subtree() {
    let mut host = Host::spawn();
    let src = host.source_dir("droppable");
    let _ = host.call(
        "new",
        serde_json::json!({
            "library": "droppable",
            "source_path": src.to_str().expect("src to str"),
        }),
    );
    write_source(
        &src,
        "thing.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let _ = host.call("commit", serde_json::json!({"library": "droppable"}));
    assert!(host.library_dir("droppable").exists());
    let resp = host.call(
        "delete",
        serde_json::json!({
            "library": "droppable",
            "source_path": src.to_str().expect("src to str"),
        }),
    );
    assert!(!has_error_path(&resp), "delete should succeed; got {resp}",);
    assert!(
        !host.library_dir("droppable").exists(),
        "lib dir should be gone after delete",
    );
    let log = git_log_subjects(&host.libraries_dir());
    assert!(
        log.iter().any(|s| s == "delete library droppable"),
        "expected delete-library commit in log; got {log:?}",
    );
}

#[test]
fn delete_missing_errors() {
    let mut host = Host::spawn();
    let resp = host.call(
        "delete",
        serde_json::json!({
            "library": "neverexisted",
            "source_path": "/some/path",
        }),
    );
    assert!(
        has_error_path(&resp),
        "delete of unknown lib should error; got {resp}",
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
