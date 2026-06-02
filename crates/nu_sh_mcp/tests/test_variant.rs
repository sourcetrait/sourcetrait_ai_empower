//! Tests for the `_test` build target -- the variant of nu_sh_mcp
//! that runs alongside the production MCP for safe live testing.
//!
//! Verifies that on the test variant:
//!   1. `info()` reports the `nu_sh_mcp_test` name (not `nu_sh_mcp`).
//!   2. XDG paths are namespaced under `nu_sh_mcp_test/` (not
//!      `nu_sh_mcp/`), so the test sandbox shares no on-disk state
//!      with a co-running production host.
//!   3. `register_library` rejects names that don't end with `_test`
//!      (defense-in-depth against corrupting production-named
//!      libraries from a misconfigured test sandbox).
//!   4. `register_library` accepts names that DO end with `_test`.
//!   5. `import_library` rejects names that don't end with `_test`
//!      (same gate as register, different entry point).

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
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
    source_root: tempfile::TempDir,
}

impl Host {
    fn spawn() -> Self {
        let host_bin = env!("CARGO_BIN_EXE_nu_sh_mcp_test");
        let worker_bin = env!("CARGO_BIN_EXE_nu_sh_mcp_test_worker");
        let data_dir = tempfile::tempdir().expect("data tempdir");
        let cache_dir = tempfile::tempdir().expect("cache tempdir");
        let client_mirror_root = tempfile::tempdir().expect("client mirror tempdir");
        let source_root = tempfile::tempdir().expect("source tempdir");
        let mut child = Command::new(host_bin)
            .env("NU_SH_MCP_WORKER_PATH", worker_bin)
            .env("XDG_DATA_HOME", data_dir.path())
            .env("XDG_CACHE_HOME", cache_dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn test host");
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
            source_root,
        };
        host.initialize();
        host
    }

    fn libraries_dir(&self) -> PathBuf {
        self.data_dir
            .path()
            .join("nu_sh_mcp_test")
            .join("libraries")
    }

    fn client_dir(&self, name: &str) -> PathBuf {
        self.client_mirror_root.path().join(name)
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
                "clientInfo": {"name": "test_variant", "version": "0.0.1"}
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

fn has_error_path(resp: &serde_json::Value) -> bool {
    resp.get("error").is_some()
        || resp
            .get("result")
            .and_then(|r| r.get("isError"))
            .and_then(|v| v.as_bool())
            == Some(true)
}

fn write_source(dir: &std::path::Path, rel: &str, contents: &str) {
    let target = dir.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(&target, contents).expect("write source");
}

#[test]
fn test_variant_info_returns_test_name() {
    let mut host = Host::spawn();
    let resp = host.call_tool("info", serde_json::json!({}));
    let result = resp
        .get("result")
        .unwrap_or_else(|| panic!("expected ok result; got {resp}"));
    let env = result
        .get("structuredContent")
        .unwrap_or_else(|| panic!("expected structuredContent; got {resp}"));
    assert_eq!(
        env["name"].as_str(),
        Some("nu_sh_mcp_test"),
        "info().name should be nu_sh_mcp_test on the _test variant; got {env}",
    );
}

#[test]
fn test_variant_xdg_paths_isolated() {
    let mut host = Host::spawn();
    let mirror = host.client_dir("foo_test");
    let resp = host.call_tool(
        "register_library",
        serde_json::json!({
            "name": "foo_test",
            "path": mirror.to_str().unwrap(),
        }),
    );
    assert!(
        !has_error_path(&resp),
        "register with _test-suffix name should succeed; got {resp}",
    );
    let lib_dir = host.libraries_dir().join("foo_test");
    assert!(
        lib_dir.exists(),
        "library dir should land under <XDG_DATA_HOME>/nu_sh_mcp_test/libraries/; \
         expected {} to exist",
        lib_dir.display(),
    );
}

#[test]
fn test_variant_register_rejects_non_test_suffix() {
    let mut host = Host::spawn();
    let mirror = host.client_dir("not_suffixed");
    let resp = host.call_tool(
        "register_library",
        serde_json::json!({
            "name": "not_suffixed",
            "path": mirror.to_str().unwrap(),
        }),
    );
    assert!(
        has_error_path(&resp),
        "register on _test variant should reject non-_test-suffix names; got {resp}",
    );
}

#[test]
fn test_variant_register_accepts_test_suffix() {
    let mut host = Host::spawn();
    let mirror = host.client_dir("hello_test");
    let resp = host.call_tool(
        "register_library",
        serde_json::json!({
            "name": "hello_test",
            "path": mirror.to_str().unwrap(),
        }),
    );
    assert!(
        !has_error_path(&resp),
        "register with _test-suffix name should succeed on _test variant; got {resp}",
    );
}

#[test]
fn test_variant_import_rejects_non_test_suffix() {
    let mut host = Host::spawn();
    let src = host.source_dir("imp_unsuffixed");
    std::fs::create_dir_all(&src).expect("mkdir src");
    write_source(&src, "mod.nu", "");
    let resp = host.call_tool(
        "import_library",
        serde_json::json!({
            "name": "imp_unsuffixed",
            "path": src.to_str().unwrap(),
        }),
    );
    assert!(
        has_error_path(&resp),
        "import on _test variant should reject non-_test-suffix names; got {resp}",
    );
}
