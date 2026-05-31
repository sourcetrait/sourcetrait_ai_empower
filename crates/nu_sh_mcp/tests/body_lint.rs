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
        "closure": "{ p: \"/home/box/proj/x\", out: 0 }",
        "functions": []
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    assert!(
        msg.contains("lint::hardcoded_variable"),
        "expected lint::hardcoded_variable token; got {msg:?}",
    );
}

#[test]
fn lint_rejects_closure_with_blacklisted_external() {
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "args": {"noop": 0},
        "closure": "{ x: (^awk '{print $1}' | str trim), out: 0 }",
        "functions": []
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    assert!(
        msg.contains("lint::blacklisted_command"),
        "expected lint::blacklisted_command token; got {msg:?}",
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
        "closure": "{ out: ($args.x + 1) }",
        "functions": []
    }));
    // Either a normal result envelope or rmcp's CallToolResult shape;
    // both wrap a JSON text content carrying the worker envelope.
    let result = resp.get("result").unwrap_or_else(|| {
        panic!("expected ok result; got {resp}");
    });
    let content = result
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or_else(|| panic!("expected text content; got {resp}"));
    let env: serde_json::Value = serde_json::from_str(content)
        .unwrap_or_else(|e| panic!("parse envelope: {e}; got {content:?}"));
    assert_eq!(env["result"]["out"].as_i64(), Some(6), "got {env}");
}

#[test]
fn lint_aggregates_multiple_violations() {
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "args": {"noop": 0},
        "closure": "\
^awk 'x'
cd \"/a/b\"
{ out: 0 }",
        "functions": []
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    // Both violations appear in one message, newline-joined.
    assert!(msg.contains("lint::blacklisted_command"), "got {msg:?}");
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
        "closure": "{ p: \"/home/box/x\", out: 0 }",
        "functions": []
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    assert!(msg.contains("lint::hardcoded_variable"), "got {msg:?}");
}

#[test]
fn lint_interact_rejects_blacklisted_external() {
    let mut host = Host::spawn();
    let resp = host.interact(serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "args": {"noop": 0},
        "closure": "{ x: (^awk 'x' | str trim), out: 0 }",
        "functions": []
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    assert!(msg.contains("lint::blacklisted_command"), "got {msg:?}");
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
fn lint_define_function_rejects_blacklisted_external() {
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
    assert!(msg.contains("lint::blacklisted_command"), "got {msg:?}");
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
    let content = result
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or_else(|| panic!("expected text content; got {resp}"));
    assert!(content.contains("\"ok\":true"), "got {content}");
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

// ----------------------------------------------------------------------------
// Slice 5.3: helper-function lint (source tag `fn <name>`)
// ----------------------------------------------------------------------------

#[test]
fn lint_helper_hardcoded_path_tagged() {
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": "x: int",
        "result_schema": "out: int",
        "args": {"x": 5},
        "closure": "{ out: ((double {x: $args.x}).out + 1) }",
        "functions": [{
            "name": "double",
            "args_schema": "x: int",
            "result_schema": "out: int",
            "body": "let p = \"/home/box/x\"; { out: ($args.x * 2) }"
        }]
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    assert!(msg.contains("lint::hardcoded_variable"), "got {msg:?}");
    assert!(msg.contains("fn double"), "got {msg:?}");
}

#[test]
fn lint_helper_blacklisted_external_tagged() {
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "args": {"noop": 0},
        "closure": "{ out: 0 }",
        "functions": [{
            "name": "evil",
            "args_schema": "noop: int",
            "result_schema": "out: int",
            "body": "let x = (^awk 'y' | str trim); { out: 0 }"
        }]
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    assert!(msg.contains("lint::blacklisted_command"), "got {msg:?}");
    assert!(msg.contains("fn evil"), "got {msg:?}");
}

#[test]
fn lint_helper_clean_closure_dirty_only_helper_flags() {
    // Closure is clean; one helper has a path; only the helper-tagged
    // violation appears.
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": "x: int",
        "result_schema": "out: int",
        "args": {"x": 1},
        "closure": "{ out: ((helper {x: $args.x}).out) }",
        "functions": [{
            "name": "helper",
            "args_schema": "x: int",
            "result_schema": "out: int",
            "body": "cd \"/x/y\"; { out: $args.x }"
        }]
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    assert!(msg.contains("lint::hardcoded_variable"), "got {msg:?}");
    assert!(msg.contains("fn helper"), "got {msg:?}");
    // Closure body has no path, so its bare-tag (no source) line should
    // not appear -- only the helper-tagged one.
    let helper_tagged_count = msg
        .lines()
        .filter(|l| l.starts_with("lint::"))
        .count();
    assert_eq!(helper_tagged_count, 1, "got {msg:?}");
}

#[test]
fn lint_helper_and_closure_both_dirty_aggregates() {
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": "noop: int",
        "result_schema": "out: int",
        "args": {"noop": 0},
        "closure": "cd \"/a/b\"; { out: 0 }",
        "functions": [{
            "name": "h1",
            "args_schema": "noop: int",
            "result_schema": "out: int",
            "body": "let p = \"/c/d\"; { out: 0 }"
        }]
    }));
    let msg = error_text(&resp)
        .unwrap_or_else(|| panic!("expected lint error; got {resp}"));
    let lines: Vec<&str> = msg.lines().filter(|l| l.starts_with("lint::")).collect();
    assert_eq!(lines.len(), 2, "got {msg:?}");
    // Closure line: no source tag.
    let closure_line = lines.iter().find(|l| !l.contains(" fn ")).unwrap_or_else(|| {
        panic!("expected one bare closure-tagged line; got {msg:?}")
    });
    assert!(
        closure_line.contains("lint::hardcoded_variable"),
        "got {closure_line:?}",
    );
    // Helper line: tagged fn h1.
    let helper_line = lines.iter().find(|l| l.contains(" fn h1")).unwrap_or_else(|| {
        panic!("expected one fn h1 line; got {msg:?}")
    });
    assert!(
        helper_line.contains("lint::hardcoded_variable"),
        "got {helper_line:?}",
    );
}

#[test]
fn lint_helper_clean_passes() {
    // Smoke that the existing smoke_4-style helper invocation still
    // works under slice 5.3 -- helpers are linted but a clean helper +
    // clean closure should round-trip an envelope.
    let mut host = Host::spawn();
    let resp = host.run(serde_json::json!({
        "args_schema": "x: int",
        "result_schema": "out: int",
        "args": {"x": 5},
        "closure": "{ out: ((double {x: $args.x}).out + 1) }",
        "functions": [{
            "name": "double",
            "args_schema": "x: int",
            "result_schema": "out: int",
            "body": "{ out: ($args.x * 2) }"
        }]
    }));
    assert!(resp.get("error").is_none(), "got {resp}");
    let result = resp.get("result").unwrap_or_else(|| {
        panic!("expected ok result; got {resp}");
    });
    let content = result
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or_else(|| panic!("expected text content; got {resp}"));
    let env: serde_json::Value = serde_json::from_str(content)
        .unwrap_or_else(|e| panic!("parse envelope: {e}; got {content:?}"));
    // double(5).out = 10; 10 + 1 = 11.
    assert_eq!(env["result"]["out"].as_i64(), Some(11), "got {env}");
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
