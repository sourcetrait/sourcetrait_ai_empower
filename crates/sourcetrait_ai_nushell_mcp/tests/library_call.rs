//! call() tests for 0.0.13 -- the final tool in slice 3.

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
                "clientInfo": {"name": "library_call", "version": "0.0.1"}
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
    resp.get("result")
        .and_then(|r| r.get("structuredContent"))
        .and_then(|sc| sc.get("error"))
        .is_some()
}

fn extract_envelope(resp: &serde_json::Value) -> Option<serde_json::Value> {
    // Success envelopes are inside structuredContent at the top level
    // (no `error` key). Error envelopes have `error`; this helper is
    // for the success path. Returns None when an error envelope sits
    // there instead.
    let sc = resp.get("result")?.get("structuredContent")?.clone();
    if sc.get("error").is_some() {
        return None;
    }
    Some(sc)
}

fn write_source(dir: &Path, rel: &str, contents: &str) {
    let target = dir.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(target, contents).unwrap();
}

fn valid_function_source(args_schema: &str, result_schema: &str, body: &str) -> String {
    format!(
        "export def call [args: record<{args_schema}>] {{\n{body}\n}}\n\nexport def resolve [args: record<{result_schema}>] {{\n    $args\n}}\n\nexport def main [args: record<{args_schema}>] {{\n    resolve (call $args)\n}}\n",
    )
}

#[test]
fn call_after_commit_returns_result() {
    let mut host = Host::spawn();
    let src = host.source_dir("calc");
    let _ = host.call_tool(
        "new",
        serde_json::json!({"library": "calc", "source_path": src.to_str().unwrap()}),
    );
    let _ = host.call_tool(
        "new",
        serde_json::json!({"library": "calc", "module_path": "math", "name": "double"}),
    );
    write_source(
        &src,
        "math/double.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let _ = host.call_tool("commit", serde_json::json!({"library": "calc"}));
    let resp = host.call_tool(
        "call",
        serde_json::json!({
            "library": "calc",
            "module_path": "math",
            "name": "double",
            "args": {"x": 7},
        }),
    );
    let env = extract_envelope(&resp).unwrap_or_else(|| panic!("call envelope; got {resp}"));
    assert_eq!(env["result"]["out"].as_i64(), Some(14));
    // No rerun_id, no version_id (HEAD-only).
    assert!(
        env.get("rerun_id").is_none(),
        "call envelope shouldn't echo rerun_id"
    );
    assert!(
        env.get("version_id").is_none(),
        "call envelope shouldn't echo version_id"
    );
}

#[test]
fn call_after_root_commit_returns_result() {
    let mut host = Host::spawn();
    let src = host.source_dir("importable");
    let _ = host.call_tool(
        "new",
        serde_json::json!({"library": "importable", "source_path": src.to_str().unwrap()}),
    );
    let _ = host.call_tool(
        "new",
        serde_json::json!({"library": "importable", "module_path": "", "name": "triple"}),
    );
    write_source(
        &src,
        "triple.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 3) }"),
    );
    let _ = host.call_tool("commit", serde_json::json!({"library": "importable"}));
    let resp = host.call_tool(
        "call",
        serde_json::json!({
            "library": "importable",
            "module_path": "",
            "name": "triple",
            "args": {"x": 11},
        }),
    );
    let env = extract_envelope(&resp).unwrap_or_else(|| panic!("call envelope; got {resp}"));
    assert_eq!(env["result"]["out"].as_i64(), Some(33));
}

#[test]
fn call_unknown_library_errors() {
    let mut host = Host::spawn();
    let resp = host.call_tool(
        "call",
        serde_json::json!({
            "library": "ghost",
            "module_path": "",
            "name": "noop",
            "args": {"noop": 0},
        }),
    );
    assert!(has_error_path(&resp));
}

#[test]
fn call_missing_function_errors() {
    let mut host = Host::spawn();
    let src = host.source_dir("partlib");
    let _ = host.call_tool(
        "new",
        serde_json::json!({"library": "partlib", "source_path": src.to_str().unwrap()}),
    );
    let resp = host.call_tool(
        "call",
        serde_json::json!({
            "library": "partlib",
            "module_path": "",
            "name": "ghost",
            "args": {"noop": 0},
        }),
    );
    assert!(has_error_path(&resp));
}

#[test]
fn call_bad_module_path_errors() {
    let mut host = Host::spawn();
    let src = host.source_dir("safelib");
    let _ = host.call_tool(
        "new",
        serde_json::json!({"library": "safelib", "source_path": src.to_str().unwrap()}),
    );
    for bad in ["../etc", "a/../b", "/abs"] {
        let resp = host.call_tool(
            "call",
            serde_json::json!({
                "library": "safelib",
                "module_path": bad,
                "name": "x",
                "args": {"n": 0},
            }),
        );
        assert!(
            has_error_path(&resp),
            "module_path {bad:?} should error; got {resp}"
        );
    }
}

#[test]
fn call_args_typecheck_failure_surfaces() {
    let mut host = Host::spawn();
    let src = host.source_dir("strictlib");
    let _ = host.call_tool(
        "new",
        serde_json::json!({"library": "strictlib", "source_path": src.to_str().unwrap()}),
    );
    let _ = host.call_tool(
        "new",
        serde_json::json!({"library": "strictlib", "module_path": "", "name": "needs_int"}),
    );
    write_source(
        &src,
        "needs_int.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    let _ = host.call_tool("commit", serde_json::json!({"library": "strictlib"}));
    // Send a string where int is expected; worker should reject at parse time.
    let resp = host.call_tool(
        "call",
        serde_json::json!({
            "library": "strictlib",
            "module_path": "",
            "name": "needs_int",
            "args": {"x": "five"},
        }),
    );
    assert!(
        has_error_path(&resp),
        "type mismatch should surface as error; got {resp}"
    );
}

#[test]
fn inspect_returns_function_doc() {
    let mut host = Host::spawn();
    let src = host.source_dir("inspectlib");
    let _ = host.call_tool(
        "new",
        serde_json::json!({"library": "inspectlib", "source_path": src.to_str().unwrap()}),
    );
    let _ = host.call_tool(
        "new",
        serde_json::json!({"library": "inspectlib", "module_path": "math", "name": "double"}),
    );
    write_source(
        &src,
        "math/double.nu",
        "export def call [args: record<x: int>] { { out: ($args.x * 2) } }\nexport def resolve [args: record<out: int>] { $args }\n# doubles its input\n# returns the doubled value\nexport def main [args: record<x: int>] { resolve (call $args) }\n",
    );
    let _ = host.call_tool("commit", serde_json::json!({"library": "inspectlib"}));
    let resp = host.call_tool(
        "inspect",
        serde_json::json!({"library": "inspectlib", "module_path": "math", "name": "double"}),
    );
    let env = extract_envelope(&resp).unwrap_or_else(|| panic!("inspect envelope; got {resp}"));
    assert_eq!(env["summary"].as_str(), Some("doubles its input"));
    assert_eq!(env["details"].as_str(), Some("returns the doubled value"));
}

#[test]
fn inspect_library_root_and_module() {
    let mut host = Host::spawn();
    let src = host.source_dir("inspectlib2");
    let _ = host.call_tool(
        "new",
        serde_json::json!({"library": "inspectlib2", "source_path": src.to_str().unwrap()}),
    );
    write_source(&src, "mod.nu", "# the inspectlib2 library\nexport module math\n");
    write_source(&src, "math/mod.nu", "# math helpers\nexport use ./double.nu\n");
    write_source(
        &src,
        "math/double.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let _ = host.call_tool("commit", serde_json::json!({"library": "inspectlib2"}));
    let lib = host.call_tool("inspect", serde_json::json!({"library": "inspectlib2"}));
    assert_eq!(
        extract_envelope(&lib).unwrap()["summary"].as_str(),
        Some("the inspectlib2 library"),
    );
    let m = host.call_tool(
        "inspect",
        serde_json::json!({"library": "inspectlib2", "module_path": "math"}),
    );
    assert_eq!(
        extract_envelope(&m).unwrap()["summary"].as_str(),
        Some("math helpers"),
    );
}

#[test]
fn inspect_undocumented_is_empty() {
    let mut host = Host::spawn();
    let src = host.source_dir("inspectlib3");
    let _ = host.call_tool(
        "new",
        serde_json::json!({"library": "inspectlib3", "source_path": src.to_str().unwrap()}),
    );
    let _ = host.call_tool(
        "new",
        serde_json::json!({"library": "inspectlib3", "module_path": "", "name": "f"}),
    );
    write_source(
        &src,
        "f.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    let _ = host.call_tool("commit", serde_json::json!({"library": "inspectlib3"}));
    let resp = host.call_tool(
        "inspect",
        serde_json::json!({"library": "inspectlib3", "module_path": "", "name": "f"}),
    );
    let env = extract_envelope(&resp).unwrap_or_else(|| panic!("inspect envelope; got {resp}"));
    assert_eq!(env["summary"].as_str(), Some(""));
    assert_eq!(env["details"].as_str(), Some(""));
}

#[test]
fn inspect_unknown_library_errors() {
    let mut host = Host::spawn();
    let resp = host.call_tool("inspect", serde_json::json!({"library": "ghost"}));
    assert!(has_error_path(&resp));
}

#[test]
fn helper_file_pruned_from_info_and_not_callable() {
    // big meta: an organizational helper file (no call/resolve sentinel) is
    // NOT an indexed call-target -> absent from info()'s function list AND not
    // callable; the real call-target beside it still works. (The pre-big-meta
    // enumerate bailed on such a file's schema parse, dropping the WHOLE
    // library from info(); the index walk prunes it cleanly instead.)
    let mut host = Host::spawn();
    let src = host.source_dir("helperlib");
    let _ = host.call_tool(
        "new",
        serde_json::json!({"library": "helperlib", "source_path": src.to_str().unwrap()}),
    );
    write_source(&src, "mod.nu", "export use ./util.nu\nexport use ./real.nu\n");
    // Organizational helper: no call/resolve sentinel -> not a call-target.
    write_source(&src, "util.nu", "export def helper [n: int] { $n * 2 }\n");
    // A real call-target beside it.
    write_source(
        &src,
        "real.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x + 1) }"),
    );
    let committed = host.call_tool("commit", serde_json::json!({"library": "helperlib"}));
    assert!(!has_error_path(&committed), "commit should succeed; got {committed}");

    // info(): the library is present and lists ONLY the call-target.
    let info = host.call_tool("info", serde_json::json!({}));
    let libs = info["result"]["structuredContent"]["libraries"]
        .as_array()
        .expect("libraries array");
    let lib = libs
        .iter()
        .find(|l| l["name"].as_str() == Some("helperlib"))
        .expect("helperlib present in info() (not dropped by the helper file)");
    let fn_names: Vec<&str> = lib["functions"]
        .as_array()
        .expect("functions")
        .iter()
        .map(|f| f["name"].as_str().expect("fn name"))
        .collect();
    assert_eq!(
        fn_names,
        vec!["real"],
        "only the call-target should be listed; got {fn_names:?}",
    );

    // call() the real target works.
    let ok = host.call_tool(
        "call",
        serde_json::json!({"library":"helperlib","module_path":"","name":"real","args":{"x":41}}),
    );
    let env = extract_envelope(&ok).unwrap_or_else(|| panic!("real call; got {ok}"));
    assert_eq!(env["result"]["out"].as_i64(), Some(42));

    // call() the helper file errors -- it is not an indexed call-target.
    let bad = host.call_tool(
        "call",
        serde_json::json!({"library":"helperlib","module_path":"","name":"util","args":{"n":5}}),
    );
    assert!(
        has_error_path(&bad),
        "an organizational helper file must NOT be callable; got {bad}",
    );
}
