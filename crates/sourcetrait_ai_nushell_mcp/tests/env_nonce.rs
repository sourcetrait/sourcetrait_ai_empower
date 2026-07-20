
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
                "clientInfo": {"name": "env_nonce", "version": "0.0.1"}
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

    fn scaffold(&mut self, namepath: &str) -> serde_json::Value {
        self.call_tool("new", serde_json::json!({"namepaths": [namepath]}))
    }

    fn call_np(&mut self, namepath: &str, args: serde_json::Value) -> serde_json::Value {
        self.call_tool("call", serde_json::json!({"namepath": namepath, "args": args}))
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn envelope(resp: &serde_json::Value) -> serde_json::Value {
    let sc = resp["result"]["structuredContent"].clone();
    assert!(
        sc.get("error").is_none(),
        "expected a success envelope; got {resp}",
    );
    sc
}

fn write_source(dir: &Path, rel: &str, contents: &str) {
    let target = dir.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(&target, contents).expect("write source");
}

#[test]
fn run_body_sees_nonce_matching_envelope() {
    let mut host = Host::spawn();
    let resp = host.call_tool(
        "run",
        serde_json::json!({
            "args_schema": {"noop": "int"},
            "result_schema": {"seen": "string"},
            "args": {"noop": 0},
            "body": "{ seen: $env.NONCE }",
        }),
    );
    let env = envelope(&resp);
    let nonce = env["nonce"].as_str().expect("envelope nonce");
    assert!(!nonce.is_empty(), "envelope nonce should be non-empty");
    assert_eq!(
        env["result"]["seen"].as_str(),
        Some(nonce),
        "run body's $env.NONCE should equal the envelope nonce; got {env}",
    );
}

#[test]
fn run_nested_helper_inherits_nonce() {
    let mut host = Host::spawn();
    let resp = host.call_tool(
        "run",
        serde_json::json!({
            "args_schema": {"noop": "int"},
            "result_schema": {"seen": "string"},
            "args": {"noop": 0},
            "body": "def grab [] { $env.NONCE }\n{ seen: (grab) }",
        }),
    );
    let env = envelope(&resp);
    let nonce = env["nonce"].as_str().expect("envelope nonce");
    assert_eq!(
        env["result"]["seen"].as_str(),
        Some(nonce),
        "a nested helper should inherit $env.NONCE; got {env}",
    );
}

#[test]
fn call_target_sees_nonce() {
    let mut host = Host::spawn();
    let src = host.source_dir("noncelib");
    let _ = host.library_new("noncelib", &src);
    let _ = host.scaffold("noncelib:m:whoami");
    write_source(
        &src,
        "m/whoami/mod.nu",
        "export def main [args: record<noop: int>]: nothing -> record<seen: string> { { seen: $env.NONCE } }\n",
    );
    let committed = host.call_tool("commit", serde_json::json!({"library": "noncelib"}));
    assert!(
        committed["result"]["structuredContent"].get("error").is_none(),
        "commit should succeed; got {committed}",
    );
    let resp = host.call_np("noncelib:m:whoami", serde_json::json!({"noop": 0}));
    let env = envelope(&resp);
    let nonce = env["nonce"].as_str().expect("envelope nonce");
    assert_eq!(
        env["result"]["seen"].as_str(),
        Some(nonce),
        "a committed call-target should read $env.NONCE; got {env}",
    );
}

#[test]
fn interact_body_sees_nonce() {
    let mut host = Host::spawn();
    let resp = host.call_tool(
        "interact",
        serde_json::json!({
            "args_schema": {"noop": "int"},
            "result_schema": {"seen": "string"},
            "args": {"noop": 0},
            "body": "{ seen: $env.NONCE }",
        }),
    );
    let env = envelope(&resp);
    let nonce = env["nonce"].as_str().expect("envelope nonce");
    assert_eq!(
        env["result"]["seen"].as_str(),
        Some(nonce),
        "interact body's $env.NONCE should equal the envelope nonce; got {env}",
    );
}

#[test]
fn interact_nonce_is_fresh_per_call_and_keeps_env_persistence() {
    let mut host = Host::spawn();
    let a = host.call_tool(
        "interact",
        serde_json::json!({
            "args_schema": {"noop": "int"},
            "result_schema": {"seen": "string"},
            "args": {"noop": 0},
            "body": "$env.KEEP = $env.NONCE\n{ seen: $env.NONCE }",
        }),
    );
    let env_a = envelope(&a);
    let nonce_a = env_a["nonce"].as_str().expect("A nonce").to_string();
    assert_eq!(env_a["result"]["seen"].as_str(), Some(nonce_a.as_str()));

    let b = host.call_tool(
        "interact",
        serde_json::json!({
            "args_schema": {"noop": "int"},
            "result_schema": {"seen": "string", "keep": "string"},
            "args": {"noop": 0},
            "body": "{ seen: $env.NONCE, keep: $env.KEEP }",
        }),
    );
    let env_b = envelope(&b);
    let nonce_b = env_b["nonce"].as_str().expect("B nonce").to_string();
    assert_ne!(nonce_a, nonce_b, "each call gets a distinct nonce");
    assert_eq!(
        env_b["result"]["seen"].as_str(),
        Some(nonce_b.as_str()),
        "B's body should see B's nonce; got {env_b}",
    );
    assert_eq!(
        env_b["result"]["keep"].as_str(),
        Some(nonce_a.as_str()),
        "A's $env.KEEP write should persist into B; got {env_b}",
    );
    assert_ne!(
        env_b["result"]["seen"].as_str(),
        env_b["result"]["keep"].as_str(),
        "B's NONCE must be fresh, not A's leftover; got {env_b}",
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
