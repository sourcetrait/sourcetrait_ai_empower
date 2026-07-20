
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
        let data_dir = tempfile::tempdir().expect("data tempdir");
        let cache_dir = tempfile::tempdir().expect("cache tempdir");
        let source_root = tempfile::tempdir().expect("source tempdir");
        let mut child = Command::new(host_bin)
            .args(["--id", "tid", "--namespace", "default"])
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

    fn libraries_dir(&self) -> PathBuf {
        self.data_dir
            .path()
            .join("sourcetrait")
            .join("nushell_mcp")
            .join("tid")
            .join("default")
            .join("libraries")
    }

    fn library_dir(&self, name: &str) -> PathBuf {
        self.libraries_dir().join("rig").join("sourcetrait").join(name)
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
                "clientInfo": {"name": "library_admin", "version": "0.0.1"}
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
            "params": {"name": tool, "arguments": _author_prefixed(tool, args)}
        });
        self.send(&req);
        self.read_id(id)
    }

    fn library_new(&mut self, name: &str, src: &Path) -> serde_json::Value {
        self.library_action("new", name, src.to_str().unwrap())
    }

    fn library_action(&mut self, action: &str, name: &str, source_dir: &str) -> serde_json::Value {
        self.call(
            "library",
            serde_json::json!({
                "action": action,
                "library": name,
                "source_dir": source_dir,
            }),
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

fn structured(resp: &serde_json::Value) -> &serde_json::Value {
    &resp["result"]["structuredContent"]
}

fn write_source(dir: &Path, rel: &str, contents: &str) {
    let target = dir.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(&target, contents).expect("write source");
}

fn valid_function_source(args_schema: &str, result_schema: &str, body: &str) -> String {
    format!(
        "export def main [args: record<{args_schema}>]: nothing -> record<{result_schema}> {{\n{body}\n}}\n",
    )
}

fn author_double_tree(src: &Path) {
    write_source(src, "mod.nu", "export module m\n");
    write_source(src, "m/mod.nu", "export use double\n");
    write_source(
        src,
        "m/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
}

#[test]
fn install_brings_shipped_source_into_mcp() {
    let mut host = Host::spawn();
    let src = host.source_dir("shiplib");
    author_double_tree(&src);
    let resp = host.library_action("install", "shiplib", src.to_str().unwrap());
    assert!(!has_error_path(&resp), "install should succeed; got {resp}");
    let summary = &structured(&resp)["summary"];
    assert!(
        !summary["added"].as_array().expect("added").is_empty(),
        "install summary should report added paths; got {summary}",
    );
    assert!(host.library_dir("shiplib").exists(), "canonical should exist");
    let called = host.call_np("shiplib:m:double", serde_json::json!({"x": 6}));
    assert_eq!(
        structured(&called)["result"]["out"].as_i64(),
        Some(12),
        "installed library should be callable; got {called}",
    );
}

#[test]
fn install_rolls_back_on_validation_failure() {
    let mut host = Host::spawn();
    let src = host.source_dir("badship");
    write_source(&src, "mod.nu", "export use thing\n");
    write_source(
        &src,
        "thing/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    let resp = host.library_action("install", "badship", src.to_str().unwrap());
    assert_eq!(
        envelope_error_kind(&resp),
        Some("library::root_function"),
        "install of an invalid source should fail with a validation diagnostic; got {resp}"
    );
    assert!(
        !host.library_dir("badship").exists(),
        "a failed install must leave nothing registered (canonical wiped)",
    );
    let re = host.library_new("badship", &src);
    assert!(
        !has_error_path(&re),
        "name should be free after rollback; got {re}",
    );
}

#[test]
fn check_reports_ok_for_clean_source() {
    let mut host = Host::spawn();
    let src = host.source_dir("checkoklib");
    let _ = host.library_new("checkoklib", &src);
    author_double_tree(&src);
    let resp = host.library_action("check", "checkoklib", src.to_str().unwrap());
    assert!(!has_error_path(&resp), "check should not error; got {resp}");
    let summary = &structured(&resp)["summary"];
    assert_eq!(summary["ok"].as_bool(), Some(true), "got {summary}");
    assert!(
        summary["errors"].as_array().expect("errors").is_empty(),
        "got {summary}"
    );
    assert!(
        summary["warnings"].as_array().expect("warnings").is_empty(),
        "got {summary}"
    );
}

#[test]
fn check_reports_structural_errors() {
    let mut host = Host::spawn();
    let src = host.source_dir("checkerrlib");
    let _ = host.library_new("checkerrlib", &src);
    write_source(&src, "mod.nu", "export use thing\n");
    write_source(
        &src,
        "thing/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    let resp = host.library_action("check", "checkerrlib", src.to_str().unwrap());
    assert!(!has_error_path(&resp), "check itself should not error; got {resp}");
    let summary = &structured(&resp)["summary"];
    assert_eq!(summary["ok"].as_bool(), Some(false), "got {summary}");
    let err_kinds: Vec<&str> = summary["errors"]
        .as_array()
        .expect("errors array")
        .iter()
        .filter_map(|e| e["kind"].as_str())
        .collect();
    assert!(!err_kinds.is_empty(), "expected >=1 error; got {summary}");
    assert!(
        err_kinds.iter().any(|k| k.starts_with("library::")),
        "errors should carry namespaced library:: kinds; got {err_kinds:?}"
    );
}

#[test]
fn check_unregistered_library_errors() {
    let mut host = Host::spawn();
    let resp = host.library_action("check", "ghostlib", "/some/path");
    assert_eq!(
        envelope_error_kind(&resp),
        Some("library::not_registered"),
        "check requires a registered library; got {resp}"
    );
}

#[test]
fn check_source_dir_mismatch_errors() {
    let mut host = Host::spawn();
    let src = host.source_dir("checkmmlib");
    let _ = host.library_new("checkmmlib", &src);
    let resp = host.library_action("check", "checkmmlib", "/wrong/path");
    assert_eq!(
        envelope_error_kind(&resp),
        Some("library::source_path_mismatch"),
        "check should cross-check source_dir; got {resp}"
    );
}

#[test]
fn uninstall_source_dir_mismatch_errors() {
    let mut host = Host::spawn();
    let src = host.source_dir("unmmlib");
    let _ = host.library_new("unmmlib", &src);
    let resp = host.library_action("uninstall", "unmmlib", "/wrong/path");
    assert_eq!(
        envelope_error_kind(&resp),
        Some("library::source_path_mismatch"),
        "uninstall should cross-check source_dir; got {resp}"
    );
    assert!(
        host.library_dir("unmmlib").exists(),
        "a rejected uninstall must leave the library registered",
    );
}

#[test]
fn invalid_action_errors() {
    let mut host = Host::spawn();
    let resp = host.library_action("frobnicate", "x", "/p");
    assert_eq!(
        envelope_error_kind(&resp),
        Some("library::invalid_action"),
        "an unknown action should error; got {resp}"
    );
}

#[test]
fn run_body_can_use_a_committed_library() {
    let mut host = Host::spawn();
    let src = host.source_dir("uselib");
    let _ = host.library_new("uselib", &src);
    write_source(&src, "mod.nu", "export module calc\n");
    write_source(&src, "calc/mod.nu", "export use double\n");
    write_source(
        &src,
        "calc/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let committed = host.call("commit", serde_json::json!({"library": "uselib"}));
    assert!(!has_error_path(&committed), "commit should succeed; got {committed}");

    let resp = host.call(
        "run",
        serde_json::json!({
            "args_schema": {},
            "result_schema": {"out": "int"},
            "args": {},
            "body": "use rig/sourcetrait/uselib/calc\nlet r = (calc double {x: 5})\n{ out: $r.out }",
        }),
    );
    assert!(
        !has_error_path(&resp),
        "a run() body should be able to `use` a committed library; got {resp}"
    );
    assert_eq!(
        structured(&resp)["result"]["out"].as_i64(),
        Some(10),
        "use-from-run should resolve + call the committed function; got {resp}",
    );
}

#[allow(dead_code)]
fn _author_prefixed(tool: &str, mut args: serde_json::Value) -> serde_json::Value {
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
