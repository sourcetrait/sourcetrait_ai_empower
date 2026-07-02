//! Tests for the `_test` build target -- the variant of nushell_mcp
//! that runs alongside the production MCP for safe live testing.
//!
//! Verifies that on the test variant:
//!   1. `info()` reports the `nushell_mcp_test` name (not `nushell_mcp`).
//!   2. XDG paths are namespaced under `sourcetrait/nushell_mcp_test/`
//!      (not `sourcetrait/nushell_mcp/`), so the test sandbox shares no
//!      on-disk state with a co-running production host.
//!   3. `library(new)` rejects library names that don't end with `_test`
//!      (defense-in-depth against corrupting production-named
//!      libraries from a misconfigured test sandbox).
//!   4. `library(new)` accepts names that DO end with `_test`.

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
        let host_bin = env!("CARGO_BIN_EXE_nushell_mcp_test");
        let worker_bin = env!("CARGO_BIN_EXE_nushell_mcp_test_worker");
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
            source_root,
        };
        host.initialize();
        host
    }

    fn libraries_dir(&self) -> PathBuf {
        self.data_dir
            .path()
            .join("sourcetrait")
            .join("nushell_mcp_test")
            .join("libraries")
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
            "params": {"name": tool, "arguments": _author_prefixed(tool, args)}
        });
        self.send(&req);
        self.read_id(id)
    }

    /// Establish a fresh library via `library(new)`.
    fn library_new(&mut self, name: &str, src: &Path) -> serde_json::Value {
        self.call_tool(
            "library",
            serde_json::json!({
                "action": "new",
                "library": name,
                "source_dir": src.to_str().unwrap(),
            }),
        )
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

fn envelope_error_kind(resp: &serde_json::Value) -> Option<&str> {
    resp.get("result")?
        .get("structuredContent")?
        .get("error")?
        .get("errors")?
        .as_array()?
        .first()?
        .get("kind")?
        .as_str()
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
        Some("nushell_mcp_test"),
        "info().name should be nushell_mcp_test on the _test variant; got {env}",
    );
}

#[test]
fn test_variant_xdg_paths_isolated() {
    let mut host = Host::spawn();
    let src = host.source_dir("foo_test");
    let resp = host.library_new("foo_test", &src);
    assert!(
        !has_error_path(&resp),
        "library(new) with _test-suffix name should succeed; got {resp}",
    );
    // Author-parented: fixtures default to author `sourcetrait`.
    let lib_dir = host.libraries_dir().join("sourcetrait").join("foo_test");
    assert!(
        lib_dir.exists(),
        "library dir should land under <XDG_DATA_HOME>/sourcetrait/nushell_mcp_test/libraries/; \
         expected {} to exist",
        lib_dir.display(),
    );
}

#[test]
fn test_variant_library_new_rejects_non_test_suffix() {
    let mut host = Host::spawn();
    let src = host.source_dir("not_suffixed");
    let resp = host.library_new("not_suffixed", &src);
    assert!(
        has_error_path(&resp),
        "library(new) on _test variant should reject non-_test-suffix names; got {resp}",
    );
    assert_eq!(
        envelope_error_kind(&resp),
        Some("library::test_suffix_required"),
        "expected test_suffix_required; got {resp}",
    );
}

#[test]
fn test_variant_library_new_accepts_test_suffix() {
    let mut host = Host::spawn();
    let src = host.source_dir("hello_test");
    let resp = host.library_new("hello_test", &src);
    assert!(
        !has_error_path(&resp),
        "library(new) with _test-suffix name should succeed on _test variant; got {resp}",
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
