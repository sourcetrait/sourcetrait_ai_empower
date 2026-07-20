
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
        let host_bin = env!("CARGO_BIN_EXE_nushell_mcp");
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
                "clientInfo": {"name": "cache_rerun", "version": "0.0.1"}
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

fn has_error(resp: &serde_json::Value) -> bool {
    resp.get("result")
        .and_then(|r| r.get("structuredContent"))
        .and_then(|sc| sc.get("error"))
        .is_some()
}

#[test]
fn run_envelope_has_nonce_and_no_rerun_id() {
    let mut host = Host::spawn();
    let resp = host.call(
        "run",
        serde_json::json!({
            "args_schema": {"x": "int"},
            "result_schema": {"out": "int"},
            "args": {"x": 1},
            "body": "{ out: ($args.x + 100) }",
        }),
    );
    let env = extract_envelope(&resp).unwrap_or_else(|| panic!("envelope; got {resp}"));
    assert!(
        env["nonce"].as_str().map(|s| !s.is_empty()).unwrap_or(false),
        "run envelope should carry a non-empty nonce; got {env}",
    );
    assert!(
        env.get("rerun_id").is_none(),
        "run envelope should no longer carry rerun_id; got {env}",
    );
}

#[test]
fn timeout_then_rerun_recovers() {
    let mut host = Host::spawn();
    let timed_out = host.call(
        "run",
        serde_json::json!({
            "args_schema": {"x": "int"},
            "result_schema": {"out": "int"},
            "args": {"x": 5},
            "body": "sleep 400ms\n{ out: ($args.x + 1) }",
            "timeout_ms": 100u64,
        }),
    );
    let err = timed_out["result"]["structuredContent"]["error"].clone();
    assert_eq!(
        err["errors"][0]["kind"].as_str(),
        Some("worker::timeout"),
        "the first run should time out; got {timed_out}",
    );
    let nonce = err["nonce"].as_str().expect("timeout envelope nonce").to_string();

    // The body was cached PRE-dispatch, so the nonce from a TIMED-OUT run is a
    // valid rerun handle -- replay with a larger timeout recovers it.
    let recovered = host.call(
        "rerun",
        serde_json::json!({ "nonce": nonce, "args": {"x": 5}, "timeout_ms": 5000u64 }),
    );
    let env = extract_envelope(&recovered)
        .unwrap_or_else(|| panic!("recovery envelope; got {recovered}"));
    assert_eq!(
        env["result"]["out"].as_i64(),
        Some(6),
        "rerun of a timed-out nonce with a bigger timeout should complete -> 6; got {env}",
    );
}

#[test]
fn rerun_by_nonce_roundtrips_with_new_args() {
    let mut host = Host::spawn();
    let first = host.call(
        "run",
        serde_json::json!({
            "args_schema": {"x": "int"},
            "result_schema": {"out": "int"},
            "args": {"x": 5},
            "body": "{ out: ($args.x * 3) }",
        }),
    );
    let first_env =
        extract_envelope(&first).unwrap_or_else(|| panic!("run 1 envelope; got {first}"));
    assert_eq!(first_env["result"]["out"].as_i64(), Some(15));
    let nonce = first_env["nonce"].as_str().expect("run nonce").to_string();

    let second = host.call(
        "rerun",
        serde_json::json!({ "nonce": nonce, "args": {"x": 7} }),
    );
    let second_env =
        extract_envelope(&second).unwrap_or_else(|| panic!("rerun envelope; got {second}"));
    assert_eq!(
        second_env["result"]["out"].as_i64(),
        Some(21),
        "rerun should replay the cached body with new args -> 7 * 3 = 21; got {:?}",
        second_env["result"],
    );
    assert!(
        second_env.get("rerun_id").is_none(),
        "rerun envelope should not include rerun_id; got {second_env}",
    );

    // The rerun's OWN nonce is itself a handle (rerun caches its body too).
    let rerun_nonce = second_env["nonce"].as_str().expect("rerun nonce").to_string();
    assert_ne!(rerun_nonce, nonce, "each eval gets a fresh nonce");
    let third = host.call(
        "rerun",
        serde_json::json!({ "nonce": rerun_nonce, "args": {"x": 2} }),
    );
    let third_env = extract_envelope(&third)
        .unwrap_or_else(|| panic!("rerun-of-rerun envelope; got {third}"));
    assert_eq!(
        third_env["result"]["out"].as_i64(),
        Some(6),
        "a rerun's own nonce must itself be rerunnable -> 2 * 3 = 6; got {third_env}",
    );
}

#[test]
fn rerun_unknown_nonce_errors() {
    let mut host = Host::spawn();
    let resp = host.call(
        "rerun",
        serde_json::json!({ "nonce": "abcDEF123456", "args": {"x": 0} }),
    );
    assert!(
        has_error(&resp),
        "a base62 nonce with no cached body should error; got {resp}",
    );
}

#[test]
fn rerun_rejects_non_base62_nonce() {
    let mut host = Host::spawn();
    let resp = host.call(
        "rerun",
        serde_json::json!({ "nonce": "../etc/passwd", "args": {"x": 0} }),
    );
    assert!(
        has_error(&resp),
        "a non-base62 nonce should error; got {resp}",
    );
}

#[test]
fn interact_envelope_has_no_rerun_id() {
    let mut host = Host::spawn();
    let resp = host.call(
        "interact",
        serde_json::json!({
            "args_schema": {"x": "int"},
            "result_schema": {"out": "int"},
            "args": {"x": 4},
            "body": "{ out: ($args.x * 2) }",
        }),
    );
    let env = extract_envelope(&resp).unwrap_or_else(|| panic!("interact envelope; got {resp}"));
    assert_eq!(env["result"]["out"].as_i64(), Some(8));
    assert!(
        env.get("rerun_id").is_none(),
        "interact envelope should not include rerun_id; got {env}",
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
