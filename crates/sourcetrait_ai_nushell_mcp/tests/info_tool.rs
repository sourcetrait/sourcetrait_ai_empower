//! info() tool surface test.
//!
//! Verifies:
//! - `info` appears in tools/list (count bumps with the new tool).
//! - envelope shape: name == "nushell_mcp", version matches the crate's
//!   CARGO_PKG_VERSION, nu_version is non-empty semver-shaped,
//!   plugins is a list of positional [name, version] pairs,
//!   libraries is the recursive library -> module -> function
//!   hierarchy (empty on a fresh host).
//! - the hierarchy for a committed library: root functions on the
//!   library node, nested module nodes, verbatim schema round-trip
//!   (including nested record<...> typedefs).
//! - the hierarchy for a hand-authored library, with `path` carrying
//!   the source directory.

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
                "clientInfo": {"name": "info_tool", "version": "0.0.1"}
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

fn write_source(dir: &std::path::Path, rel: &str, contents: &str) {
    let target = dir.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(&target, contents).expect("write source");
}

fn valid_function_source(args_schema: &str, result_schema: &str, body: &str) -> String {
    format!(
        "export def call [args: record<{args_schema}>] {{\n{body}\n}}\n\nexport def resolve [args: record<{result_schema}>] {{\n    $args\n}}\n\nexport def main [args: record<{args_schema}>] {{\n    resolve (call $args)\n}}\n",
    )
}

#[test]
fn info_tool_in_list() {
    let mut host = Host::spawn();
    let resp = host.list_tools();
    let tools = resp["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().expect("tool name"))
        .collect();
    assert!(
        names.contains(&"info"),
        "info missing from tools/list; got {names:?}",
    );
}

#[test]
fn info_returns_static_server_state() {
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
        Some("nushell_mcp"),
        "name should be nushell_mcp; got {env}",
    );

    let version = env["version"].as_str().expect("version field");
    assert_eq!(
        version,
        env!("CARGO_PKG_VERSION"),
        "version should match crate CARGO_PKG_VERSION",
    );

    let nu_version = env["nu_version"].as_str().expect("nu_version field");
    assert!(
        !nu_version.is_empty() && nu_version.contains('.'),
        "nu_version should be non-empty + semver-shaped; got {nu_version:?}",
    );

    let plugins = env["plugins"]
        .as_array()
        .expect("plugins should be an array");
    for p in plugins {
        let entry = p
            .as_array()
            .expect("each plugin is a positional [name, version] pair");
        assert_eq!(
            entry.len(),
            2,
            "plugin entry is a 2-element [name, version]; got {entry:?}",
        );
        let name = entry[0].as_str().expect("plugin name is a string");
        assert!(!name.is_empty(), "plugin name should be non-empty");
        // entry[1] is the version: a string when the plugin reports one,
        // null otherwise (the positional slot is always present).
        assert!(
            entry[1].is_string() || entry[1].is_null(),
            "plugin version slot should be string-or-null; got {:?}",
            entry[1],
        );
    }

    let libraries = env["libraries"]
        .as_array()
        .expect("libraries should be an array");
    assert!(
        libraries.is_empty(),
        "fresh host should report no libraries; got {libraries:?}",
    );
}

#[test]
fn info_lists_committed_library_hierarchy() {
    let mut host = Host::spawn();
    let src = host.source_dir("treelib");
    let est = host.call_tool(
        "new",
        serde_json::json!({"library": "treelib", "source_path": src.to_str().unwrap()}),
    );
    assert!(
        est["result"]["structuredContent"].get("error").is_none(),
        "establish failed: {est}",
    );
    // Root function (library node), one in `alpha`, one in `alpha/beta`.
    // b1 carries a NESTED record typedef to exercise balanced extraction.
    // info() reads args_schema from `main`'s positional and result_schema
    // from `resolve`'s positional, so valid_function_source(args, result, ..)
    // round-trips both. The nu record-inner forms below render to the JSON
    // schemas asserted after commit.
    for (module_path, name, args_inner, result_inner, body) in [
        ("", "rootfn", "x: int", "out: int", "{ out: ($args.x + 1) }"),
        (
            "alpha",
            "a1",
            "s: string",
            "len: int",
            "{ len: ($args.s | str length) }",
        ),
        (
            "alpha/beta",
            "b1",
            "x: int, t: record<y: string, n: list<int>>",
            "out: record<y: string>",
            "{ out: { y: $args.t.y } }",
        ),
    ] {
        let scaffold = host.call_tool(
            "new",
            serde_json::json!({
                "library": "treelib",
                "module_path": module_path,
                "name": name,
            }),
        );
        assert!(
            scaffold["result"]["structuredContent"]
                .get("error")
                .is_none(),
            "scaffold {name} failed: {scaffold}",
        );
        let rel = if module_path.is_empty() {
            format!("{name}.nu")
        } else {
            format!("{module_path}/{name}.nu")
        };
        write_source(
            &src,
            &rel,
            &valid_function_source(args_inner, result_inner, body),
        );
    }
    let committed = host.call_tool("commit", serde_json::json!({"library": "treelib"}));
    assert!(
        committed["result"]["structuredContent"]
            .get("error")
            .is_none(),
        "commit failed: {committed}",
    );

    let resp = host.call_tool("info", serde_json::json!({}));
    let env = resp["result"]
        .get("structuredContent")
        .unwrap_or_else(|| panic!("expected structuredContent; got {resp}"));
    let libs = env["libraries"].as_array().expect("libraries array");
    assert_eq!(libs.len(), 1, "got {libs:?}");
    let lib = &libs[0];
    assert_eq!(lib["name"].as_str(), Some("treelib"));
    assert_eq!(
        lib["path"].as_str(),
        src.to_str(),
        "path should be the meta source_path",
    );

    // Root function on the library node.
    let root_fns = lib["functions"].as_array().expect("library functions");
    assert_eq!(root_fns.len(), 1, "got {root_fns:?}");
    assert_eq!(root_fns[0]["name"].as_str(), Some("rootfn"));
    assert_eq!(root_fns[0]["args_schema"], serde_json::json!({"x": "int"}));
    assert_eq!(
        root_fns[0]["result_schema"],
        serde_json::json!({"out": "int"})
    );

    // alpha -> { functions: [a1], submodules: [beta -> { functions: [b1] }] }
    let modules = lib["modules"].as_array().expect("library modules");
    assert_eq!(modules.len(), 1, "got {modules:?}");
    let alpha = &modules[0];
    assert_eq!(alpha["name"].as_str(), Some("alpha"));
    let alpha_fns = alpha["functions"].as_array().expect("alpha functions");
    assert_eq!(alpha_fns.len(), 1);
    assert_eq!(alpha_fns[0]["name"].as_str(), Some("a1"));
    let alpha_subs = alpha["submodules"].as_array().expect("alpha submodules");
    assert_eq!(alpha_subs.len(), 1, "got {alpha_subs:?}");
    let beta = &alpha_subs[0];
    assert_eq!(beta["name"].as_str(), Some("beta"));
    assert!(
        beta["submodules"]
            .as_array()
            .expect("beta submodules")
            .is_empty()
    );
    let beta_fns = beta["functions"].as_array().expect("beta functions");
    assert_eq!(beta_fns.len(), 1);
    assert_eq!(beta_fns[0]["name"].as_str(), Some("b1"));
    // Verbatim round-trip of the nested typedef.
    assert_eq!(
        beta_fns[0]["args_schema"],
        serde_json::json!({"x": "int", "t": {"y": "string", "n": ["int"]}}),
    );
    assert_eq!(
        beta_fns[0]["result_schema"],
        serde_json::json!({"out": {"y": "string"}}),
    );
}

#[test]
fn info_lists_hand_authored_library_hierarchy() {
    let mut host = Host::spawn();
    let src = host.source_dir("implib");
    // Establish the library, then hand-author the full source tree and
    // commit it (commit re-reads the recorded source_path).
    let est = host.call_tool(
        "new",
        serde_json::json!({"library": "implib", "source_path": src.to_str().unwrap()}),
    );
    assert!(
        est["result"]["structuredContent"].get("error").is_none(),
        "establish failed: {est}",
    );
    write_source(&src, "mod.nu", "export module math\n");
    write_source(&src, "math/mod.nu", "export use ./double.nu\n");
    write_source(
        &src,
        "math/double.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );

    let committed = host.call_tool("commit", serde_json::json!({"library": "implib"}));
    assert!(
        committed["result"]["structuredContent"]
            .get("error")
            .is_none(),
        "commit failed: {committed}",
    );

    let resp = host.call_tool("info", serde_json::json!({}));
    let env = resp["result"]
        .get("structuredContent")
        .unwrap_or_else(|| panic!("expected structuredContent; got {resp}"));
    let libs = env["libraries"].as_array().expect("libraries array");
    assert_eq!(libs.len(), 1, "got {libs:?}");
    let lib = &libs[0];
    assert_eq!(lib["name"].as_str(), Some("implib"));
    assert_eq!(
        lib["path"].as_str(),
        src.to_str(),
        "path should be the source directory",
    );
    assert!(lib["functions"].as_array().expect("root fns").is_empty());
    let modules = lib["modules"].as_array().expect("modules");
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0]["name"].as_str(), Some("math"));
    assert!(
        modules[0]["submodules"]
            .as_array()
            .expect("math submodules")
            .is_empty(),
    );
    let math_fns = modules[0]["functions"].as_array().expect("math fns");
    assert_eq!(math_fns.len(), 1);
    assert_eq!(math_fns[0]["name"].as_str(), Some("double"));
    assert_eq!(math_fns[0]["args_schema"], serde_json::json!({"x": "int"}));
    assert_eq!(
        math_fns[0]["result_schema"],
        serde_json::json!({"out": "int"})
    );
}
