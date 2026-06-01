//! Slice 5.1 + 5.2 integration tests for the AST body linter. Each
//! handler that accepts agent-authored body source short-circuits on
//! lint violations with the agent-fixable `lint::<class> [L:C]`
//! report shape (with optional ` mod <rel_path>` source tag for
//! library import paths). Helper-function lint lands in slice 5.3 --
//! not covered here. `rerun()` does NOT re-lint per the_user
//! 2026-05-31 design call (trust the cache).

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
        let host_bin = env!("CARGO_BIN_EXE_nu_sh_mcp");
        let worker_bin = env!("CARGO_BIN_EXE_nu_sh_mcp_worker");
        let data_dir = tempfile::tempdir().expect("data tempdir");
        let cache_dir = tempfile::tempdir().expect("cache tempdir");
        let source_root = tempfile::tempdir().expect("source tempdir");
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
                "clientInfo": {"name": "body_lint", "version": "0.0.1"}
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

    fn run(&mut self, args: serde_json::Value) -> serde_json::Value {
        self.call_tool("run", args)
    }

    fn interact(&mut self, args: serde_json::Value) -> serde_json::Value {
        self.call_tool("interact", args)
    }

    fn register(&mut self, name: &str, path: &str) -> serde_json::Value {
        self.call_tool("register_library", serde_json::json!({
            "name": name,
            "path": path,
        }))
    }

    fn define_function(&mut self, args: serde_json::Value) -> serde_json::Value {
        self.call_tool("define_function", args)
    }

    fn import(&mut self, name: &str, path: &str) -> serde_json::Value {
        self.call_tool("import_library", serde_json::json!({
            "name": name,
            "path": path,
        }))
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn error_text(resp: &serde_json::Value) -> Option<String> {
    // rmcp surfaces Err(ErrorData::invalid_params) as a JSON-RPC error
    // object at the top level: {"error": {"code": -32602, "message": ...}}.
    resp.get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .map(str::to_string)
}

#[test]
fn lint_rejects_closure_with_hardcoded_path() {
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "args": {"noop": 0},
        "body": "{ p: \"/home/box/proj/x\", out: 0 }",
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    assert!(
        msg.contains("lint::hardcoded_variable"),
        "expected lint::hardcoded_variable token; got {msg:?}",
    );
}

#[test]
fn lint_rejects_closure_with_denied_external() {
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "args": {"noop": 0},
        "body": "{ x: (^awk '{print $1}' | str trim), out: 0 }",
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    assert!(
        msg.contains("lint::denied_command"),
        "expected lint::denied_command token; got {msg:?}",
    );
}

#[test]
fn lint_passes_clean_closure() {
    // No lint violations -> reaches the worker -> normal envelope path.
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": "x: int",
        "result_schema": "out: int",
        "args": {"x": 5},
        "body": "{ out: ($args.x + 1) }",
    }));
    // C2: `run` emits structured_content only (no content[] mirror).
    let result = resp.get("result").unwrap_or_else(|| {
        panic!("expected ok result; got {resp}");
    });
    let env = result
        .get("structuredContent")
        .cloned()
        .unwrap_or_else(|| panic!("expected structuredContent; got {resp}"));
    assert_eq!(env["result"]["out"].as_i64(), Some(6), "got {env}");
}

#[test]
fn lint_aggregates_multiple_violations() {
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "args": {"noop": 0},
        "body": "\
^awk 'x'
cd \"/a/b\"
{ out: 0 }",
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    // Both violations appear in one message, newline-joined.
    assert!(msg.contains("lint::denied_command"), "got {msg:?}");
    assert!(msg.contains("lint::hardcoded_variable"), "got {msg:?}");
}

// ----------------------------------------------------------------------------
// Slice 5.2: interact() body lint
// ----------------------------------------------------------------------------

#[test]
fn lint_interact_rejects_hardcoded_path() {
    let mut host = Host::spawn();
    let resp = host.interact(serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "args": {"noop": 0},
        "body": "{ p: \"/home/box/x\", out: 0 }",
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    assert!(msg.contains("lint::hardcoded_variable"), "got {msg:?}");
}

#[test]
fn lint_interact_rejects_denied_external() {
    let mut host = Host::spawn();
    let resp = host.interact(serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "args": {"noop": 0},
        "body": "{ x: (^awk 'x' | str trim), out: 0 }",
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    assert!(msg.contains("lint::denied_command"), "got {msg:?}");
}

// ----------------------------------------------------------------------------
// Slice 5.2: define_function body lint
// ----------------------------------------------------------------------------

#[test]
fn lint_define_function_rejects_hardcoded_path() {
    let mut host = Host::spawn();
    let mirror = host.source_dir("mirror");
    std::fs::create_dir_all(&mirror).expect("mkdir mirror");
    let reg = host.register("lib1", mirror.to_str().unwrap());
    assert!(reg.get("error").is_none(), "register failed: {reg}");
    let resp = host.define_function(serde_json::json!({
        "library": "lib1",
        "module_path": "",
        "name": "bad",
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "body": "{ p: \"/home/box/x\", out: 0 }"
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    assert!(msg.contains("lint::hardcoded_variable"), "got {msg:?}");
}

#[test]
fn lint_define_function_rejects_denied_external() {
    let mut host = Host::spawn();
    let mirror = host.source_dir("mirror2");
    std::fs::create_dir_all(&mirror).expect("mkdir mirror");
    let reg = host.register("lib2", mirror.to_str().unwrap());
    assert!(reg.get("error").is_none(), "register failed: {reg}");
    let resp = host.define_function(serde_json::json!({
        "library": "lib2",
        "module_path": "",
        "name": "bad",
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "body": "{ x: (^rm -rf /; 0) }"
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    assert!(msg.contains("lint::denied_command"), "got {msg:?}");
}

#[test]
fn lint_define_function_passes_clean_body() {
    let mut host = Host::spawn();
    let mirror = host.source_dir("mirror3");
    std::fs::create_dir_all(&mirror).expect("mkdir mirror");
    let reg = host.register("lib3", mirror.to_str().unwrap());
    assert!(reg.get("error").is_none(), "register failed: {reg}");
    let resp = host.define_function(serde_json::json!({
        "library": "lib3",
        "module_path": "",
        "name": "good",
        "args_schema": "x: int",
        "result_schema": "out: int",
        "body": "{ out: ($args.x + 1) }"
    }));
    // Expect ok envelope, not an error.
    assert!(resp.get("error").is_none(), "define rejected: {resp}");
    let result = resp.get("result").unwrap_or_else(|| {
        panic!("expected ok result; got {resp}");
    });
    // C3: define_function emits structured_content only.
    let env = result
        .get("structuredContent")
        .unwrap_or_else(|| panic!("expected structuredContent; got {resp}"));
    assert_eq!(env["ok"].as_bool(), Some(true), "got {env}");
}

// ----------------------------------------------------------------------------
// Slice 5.2: import_library body lint (source-tagged with `mod <rel_path>`)
// ----------------------------------------------------------------------------

fn write_library_with_path_in_main(root: &std::path::Path) {
    std::fs::create_dir_all(root).expect("mkdir lib root");
    let mod_nu = "export use ./bad.nu\n";
    std::fs::write(root.join("mod.nu"), mod_nu).expect("write mod.nu");
    // Hardcoded path inside main's body; resolve passthrough.
    let bad_nu = "\
export def main [args: record<noop: int>] {
    cd \"/home/box/proj/x\"
    { out: 0 }
}

export def resolve [args: record<out: int>] {
    $args
}
";
    std::fs::write(root.join("bad.nu"), bad_nu).expect("write bad.nu");
}

#[test]
fn lint_import_library_reports_main_body_violation() {
    let mut host = Host::spawn();
    let lib = host.source_dir("liblint1");
    write_library_with_path_in_main(&lib);
    let resp = host.import("liblint1", lib.to_str().unwrap());
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    // Lint section is present and source-tagged with `mod bad.nu`.
    assert!(msg.contains("lint::hardcoded_variable"), "got {msg:?}");
    assert!(msg.contains("mod bad.nu"), "got {msg:?}");
}

#[test]
fn lint_import_library_combines_structural_and_lint() {
    let mut host = Host::spawn();
    let lib = host.source_dir("liblint2");
    std::fs::create_dir_all(&lib).expect("mkdir lib");
    // Structural violation: mod.nu has an inline def.
    let mod_nu = "\
export use ./bad.nu
def helper [] { 1 }
";
    std::fs::write(lib.join("mod.nu"), mod_nu).expect("write mod.nu");
    // Lint violation: bad.nu has hardcoded path in main.
    let bad_nu = "\
export def main [args: record<noop: int>] {
    cd \"/home/box/x\"
    { out: 0 }
}

export def resolve [args: record<out: int>] {
    $args
}
";
    std::fs::write(lib.join("bad.nu"), bad_nu).expect("write bad.nu");
    let resp = host.import("liblint2", lib.to_str().unwrap());
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected error; got {resp}"));
    // Structural section header + bullet.
    assert!(msg.contains("validation failed:"), "got {msg:?}");
    assert!(msg.contains("mod.nu"), "got {msg:?}");
    // Lint section.
    assert!(msg.contains("lint::hardcoded_variable"), "got {msg:?}");
    assert!(msg.contains("mod bad.nu"), "got {msg:?}");
}
