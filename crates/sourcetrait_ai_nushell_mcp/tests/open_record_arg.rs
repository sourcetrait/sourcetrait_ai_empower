//! Open-record args (`record<>`) tests (item 27).
//!
//! An empty `record<>` is allowed as an OPEN record wherever a type appears in
//! an ARGS schema (record field, oneof member, table column) - never in a result
//! schema, and never as a bare list element (`[{}]` is an empty table). Covers
//! both the run() submit path (json->nu) and the library commit/call path
//! (nu->json, read from the author's source annotation).

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
        self.send(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "open_record_arg", "version": "0.0.1"}
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

    fn call(&mut self, tool: &str, args: serde_json::Value) -> serde_json::Value {
        let id = self.next_id();
        self.send(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": tool, "arguments": _author_prefixed(tool, args)}
        }));
        self.read_id(id)
    }

    fn library_new(&mut self, name: &str, src: &Path) -> serde_json::Value {
        self.call(
            "library",
            serde_json::json!({"action": "new", "library": name, "source_dir": src.to_str().unwrap()}),
        )
    }

    fn call_np(&mut self, namepath: &str, args: serde_json::Value) -> serde_json::Value {
        self.call("call", serde_json::json!({"namepath": namepath, "args": args}))
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn envelope_error(resp: &serde_json::Value) -> Option<&serde_json::Value> {
    resp.get("result")?.get("structuredContent")?.get("error")
}

fn has_error_path(resp: &serde_json::Value) -> bool {
    envelope_error(resp).is_some()
}

fn success(resp: &serde_json::Value) -> serde_json::Value {
    let sc = resp["result"]["structuredContent"].clone();
    assert!(
        sc.get("error").is_none(),
        "expected a success envelope; got {resp}",
    );
    sc
}

fn error_message(resp: &serde_json::Value) -> String {
    envelope_error(resp)
        .map(|e| e.to_string())
        .unwrap_or_else(|| resp.to_string())
}

fn write_source(dir: &Path, rel: &str, contents: &str) {
    let target = dir.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(&target, contents).expect("write source");
}

#[test]
fn run_binds_open_record_arg() {
    // args_schema `{fill: {}}` -> positional `record<fill: record<>>`; an
    // arbitrary record binds and is readable in the body.
    let mut host = Host::spawn();
    let resp = host.call(
        "run",
        serde_json::json!({
            "args_schema": {"fill": {}},
            "result_schema": {"cols": "int"},
            "args": {"fill": {"a": 1, "b": "x"}},
            "body": "{ cols: ($args.fill | columns | length) }",
        }),
    );
    let env = success(&resp);
    assert_eq!(
        env["result"]["cols"].as_i64(),
        Some(2),
        "open-record arg should bind an arbitrary record; got {env}",
    );
}

#[test]
fn run_binds_empty_open_record_arg() {
    // The open record also accepts an empty record.
    let mut host = Host::spawn();
    let resp = host.call(
        "run",
        serde_json::json!({
            "args_schema": {"fill": {}},
            "result_schema": {"cols": "int"},
            "args": {"fill": {}},
            "body": "{ cols: ($args.fill | columns | length) }",
        }),
    );
    let env = success(&resp);
    assert_eq!(env["result"]["cols"].as_i64(), Some(0), "got {env}");
}

#[test]
fn run_rejects_non_record_open_arg() {
    // A NON-record value for a `record<>` field still fails the positional check.
    let mut host = Host::spawn();
    let resp = host.call(
        "run",
        serde_json::json!({
            "args_schema": {"fill": {}},
            "result_schema": {"cols": "int"},
            "args": {"fill": 5},
            "body": "{ cols: 0 }",
        }),
    );
    assert!(
        has_error_path(&resp),
        "a non-record `fill` should be rejected; got {resp}",
    );
}

#[test]
fn run_result_open_record_still_denied() {
    // The relaxation is args-only: an empty `{}` in a RESULT schema still denies.
    let mut host = Host::spawn();
    let resp = host.call(
        "run",
        serde_json::json!({
            "args_schema": {"noop": "int"},
            "result_schema": {"fill": {}},
            "args": {"noop": 0},
            "body": "{ fill: {} }",
        }),
    );
    assert!(has_error_path(&resp), "result open record should deny; got {resp}");
    let msg = error_message(&resp);
    assert!(
        msg.contains("nested empty record"),
        "expected the nested-empty-record denial; got {msg}",
    );
}

#[test]
fn commit_inspect_and_call_open_record_arg_field() {
    // The library commit/call path: a call-target whose `main` takes an open
    // record arg field commits, indexes with `fill: {}`, and is callable with an
    // arbitrary record. This is the empower `liquid/soak/dir` `fill: record<>`
    // shape (item 27).
    let mut host = Host::spawn();
    let src = host.source_dir("openlib");
    let _ = host.library_new("openlib", &src);
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export module soak\n");
    write_source(
        &src,
        "m/soak/mod.nu",
        "export def main [args: record<x: int, fill: record<>>]: nothing -> record<sum: int, fillcols: int> {\n    { sum: $args.x, fillcols: ($args.fill | columns | length) }\n}\n",
    );
    let committed = host.call("commit", serde_json::json!({"library": "openlib"}));
    assert!(
        !has_error_path(&committed),
        "a call-target with a `record<>` arg field should commit; got {committed}",
    );

    // inspect: the args schema carries `fill: {}` (open record) verbatim.
    let inspected = host.call(
        "inspect",
        serde_json::json!({"namepath": "openlib:m:soak"}),
    );
    let env = success(&inspected);
    assert_eq!(
        env["args_schema"],
        serde_json::json!({"x": "int", "fill": {}}),
        "inspect should show the open-record arg field; got {env}",
    );
    assert_eq!(
        env["result_schema"],
        serde_json::json!({"sum": "int", "fillcols": "int"}),
    );

    // call: an arbitrary `fill` record binds and is usable.
    let called = host.call_np("openlib:m:soak", serde_json::json!({"x": 5, "fill": {"a": 1, "b": 2, "c": 3}}));
    let cenv = success(&called);
    assert_eq!(cenv["result"]["sum"].as_i64(), Some(5), "got {cenv}");
    assert_eq!(cenv["result"]["fillcols"].as_i64(), Some(3), "got {cenv}");
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
