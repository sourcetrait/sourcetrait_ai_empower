
use std::io::{BufRead, BufReader, Write};
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
}

impl Host {
    fn spawn() -> Self {
        let host_bin = env!("CARGO_BIN_EXE_grammar_mcp");
        let data_dir = tempfile::tempdir().expect("data tempdir");
        let cache_dir = tempfile::tempdir().expect("cache tempdir");
        let mut child = Command::new(host_bin)
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
        };
        host.initialize();
        host
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
                "clientInfo": {"name": "smoke", "version": "0.0.1"}
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

    fn run(&mut self, args: serde_json::Value) -> serde_json::Value {
        let id = self.next_id();
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": "run", "arguments": args}
        });
        self.send(&req);
        self.read_id(id)
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
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn extract_envelope(call_response: &serde_json::Value) -> Option<serde_json::Value> {
    let result = call_response.get("result")?;
    if let Some(sc) = result.get("structuredContent") {
        return Some(sc.clone());
    }
    let content = result.get("content")?.as_array()?;
    let text = content.first()?.get("text")?.as_str()?;
    serde_json::from_str(text).ok()
}

#[test]
fn smoke_2_runtime_arg_typecheck_error() {
    let mut host = Host::spawn();
    let args = serde_json::json!({
        "args_schema": {"x": "int"},
        "result_schema": {"out": "int"},
        "args": {"x": "five"},
        "body": "{ out: ($args.x + 1) }",
    });
    let resp = host.run(args);
    let has_error_path = resp
        .get("result")
        .and_then(|r| r.get("structuredContent"))
        .and_then(|sc| sc.get("error"))
        .is_some();
    assert!(
        has_error_path,
        "expected parse-time arg mismatch to surface as error; got {resp}",
    );
}

#[test]
fn smoke_3_runtime_result_typecheck_error() {
    let mut host = Host::spawn();
    let args = serde_json::json!({
        "args_schema": {"x": "int"},
        "result_schema": {"out": "int"},
        "args": {"x": 5},
        "body": "{ out: \"five\" }",
    });
    let resp = host.run(args);
    let has_error_path = resp
        .get("result")
        .and_then(|r| r.get("structuredContent"))
        .and_then(|sc| sc.get("error"))
        .is_some();
    assert!(
        has_error_path,
        "expected runtime result mismatch to surface as error; got {resp}",
    );
}

#[test]
fn smoke_5_external_command() {
    let mut host = Host::spawn();
    let args = serde_json::json!({
        "args_schema": {"noop": "int"},
        "result_schema": {"out": "string"},
        "args": {"noop": 0},
        "body": "{ out: (^printf hello | str trim) }",
    });
    let resp = host.run(args);
    let envelope =
        extract_envelope(&resp).unwrap_or_else(|| panic!("expected envelope; got {resp}"));
    assert_eq!(
        envelope["result"]["out"].as_str(),
        Some("hello"),
        "expected {{out: \"hello\"}}; got {:?}",
        envelope["result"],
    );
}

#[test]
fn smoke_6_exit_decl_is_unreachable() {
    let mut host = Host::spawn();
    let args = serde_json::json!({
        "args_schema": {"noop": "int"},
        "result_schema": {"out": "int"},
        "args": {"noop": 0},
        "body": "{ out: (exit 1; 0) }",
    });
    let resp = host.run(args);
    let err = resp
        .get("result")
        .and_then(|r| r.get("structuredContent"))
        .and_then(|sc| sc.get("error"))
        .unwrap_or_else(|| {
            panic!("expected `exit` to be a disabled decl, not a host-fatal process exit; got {resp}")
        });
    let msg = err["errors"][0]["message"].as_str().unwrap_or_default();
    assert!(
        msg.contains("disabled"),
        "`exit` must be shadowed by an erroring decl in-process (no host-fatal exit); got {err}",
    );
}

#[test]
fn smoke_9_timeout_fires() {
    let mut host = Host::spawn();
    let args = serde_json::json!({
        "args_schema": {"noop": "int"},
        "result_schema": {"out": "int"},
        "args": {"noop": 0},
        "body": "sleep 5sec\n{ out: 0 }",
        "timeout_ms": 200u64
    });
    let resp = host.run(args);
    let env = resp
        .get("result")
        .and_then(|r| r.get("structuredContent"))
        .and_then(|sc| sc.get("error"))
        .unwrap_or_else(|| panic!("expected error envelope; got {resp}"));
    assert_eq!(
        env["errors"][0]["kind"].as_str(),
        Some("thread::timeout"),
        "got {env}"
    );
    assert!(
        env["errors"][0]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("200"),
        "timeout message should carry the ms; got {env}"
    );
    assert!(env["nonce"].as_str().is_some(), "expected nonce; got {env}");
    let args2 = serde_json::json!({
        "args_schema": {"x": "int"},
        "result_schema": {"out": "int"},
        "args": {"x": 7},
        "body": "{ out: ($args.x + 1) }",
    });
    let resp2 = host.run(args2);
    let env = extract_envelope(&resp2).unwrap_or_else(|| panic!("expected envelope; got {resp2}"));
    assert_eq!(env["result"]["out"].as_i64(), Some(8), "got {env}");
}

#[test]
fn smoke_10_processes_empty_when_idle() {
    let mut host = Host::spawn();
    let resp = host.call_tool("processes", serde_json::json!({}));
    let result = resp.get("result").unwrap_or_else(|| {
        panic!("expected ok result; got {resp}");
    });
    let env = result
        .get("structuredContent")
        .unwrap_or_else(|| panic!("expected structuredContent; got {resp}"));
    let list = env["processes"]
        .as_array()
        .unwrap_or_else(|| panic!("expected processes array; got {env}"));
    assert!(list.is_empty(), "expected empty in-flight; got {list:?}");
}

#[test]
fn smoke_11_kill_unknown_nonce_silent_ok() {
    let mut host = Host::spawn();
    let resp = host.call_tool("kill", serde_json::json!({"nonce": "doesnotexist"}));
    let result = resp.get("result").unwrap_or_else(|| {
        panic!("expected ok result; got {resp}");
    });
    assert!(
        result.get("structuredContent").is_none(),
        "no-return tools should not emit structuredContent; got {result}",
    );
}

#[test]
fn smoke_8_plugin_path_resolves() {
    let mut host = Host::spawn();
    let args = serde_json::json!({
        "args_schema": {"noop": "int"},
        "result_schema": {"path": "string"},
        "args": {"noop": 0},
        "body": "{ path: $nu.plugin-path }",
    });
    let resp = host.run(args);
    let envelope =
        extract_envelope(&resp).unwrap_or_else(|| panic!("expected envelope; got {resp}"));
    let path = envelope["result"]["path"]
        .as_str()
        .unwrap_or_else(|| panic!("expected string; got {:?}", envelope["result"]));
    assert!(
        path.ends_with("plugin.msgpackz"),
        "expected $nu.plugin-path to end with plugin.msgpackz; got {path:?}",
    );
}

#[test]
fn smoke_12_tls_crypto_provider_installed() {
    let mut host = Host::spawn();
    let args = serde_json::json!({
        "args_schema": {"noop": "int"},
        "result_schema": {"out": "string"},
        "args": {"noop": 0},
        "body": "{ out: (try { http get 'https://127.0.0.1:9' | to text } catch {|e| $e.msg }) }",
    });
    let resp = host.run(args);
    let envelope =
        extract_envelope(&resp).unwrap_or_else(|| panic!("expected envelope; got {resp}"));
    let out = envelope["result"]["out"].as_str().unwrap_or_default();
    assert!(
        !out.is_empty(),
        "expected a connection error message; got empty out",
    );
    assert!(
        !out.to_lowercase().contains("crypto provider"),
        "provider should be installed; got {out:?}",
    );
}

#[test]
fn smoke_7_multi_call_stability_and_scoping() {
    let mut host = Host::spawn();
    for i in 0..10 {
        let args = serde_json::json!({
            "args_schema": {"x": "int"},
            "result_schema": {"out": "int"},
            "args": {"x": i as i64},
            "body": "{ out: ($args.x + 100) }",
        });
        let resp = host.run(args);
        let envelope = extract_envelope(&resp)
            .unwrap_or_else(|| panic!("call {i}: expected envelope; got {resp}"));
        let expected = (i + 100) as i64;
        assert_eq!(
            envelope["result"]["out"].as_i64(),
            Some(expected),
            "call {i}: expected {{out: {expected}}}; got {:?}",
            envelope["result"],
        );
    }
    let intro = serde_json::json!({
        "args_schema": {"noop": "int"},
        "result_schema": {"leaked": "int"},
        "args": {"noop": 0},
        "body": "{ leaked: (scope commands | where name == \"__run\" | length) }",
    });
    let resp = host.run(intro);
    let envelope =
        extract_envelope(&resp).unwrap_or_else(|| panic!("intro: expected envelope; got {resp}"));
    assert_eq!(
        envelope["result"]["leaked"].as_i64(),
        Some(0),
        "do-block scoping should keep __run out of the persistent \
         EngineState; got {:?}",
        envelope["result"],
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
