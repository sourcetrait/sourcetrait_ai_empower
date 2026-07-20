
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Host {
    child: Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl Host {
    fn spawn_with(args: &[&str], envs: &[(&str, &str)], data: &Path, cache: &Path) -> Self {
        let host_bin = env!("CARGO_BIN_EXE_nushell_mcp");
        let mut command = Command::new(host_bin);
        command
            .args(args)
            .env("XDG_DATA_HOME", data)
            .env("XDG_CACHE_HOME", cache)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        for (k, v) in envs {
            command.env(k, v);
        }
        let mut child = command.spawn().expect("spawn host");
        let stdin = child.stdin.take().expect("host stdin");
        let stdout = BufReader::new(child.stdout.take().expect("host stdout"));
        let mut host = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        };
        host.initialize();
        host
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
                "clientInfo": {"name": "id_namespace", "version": "0.0.1"}
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
            "params": {"name": tool, "arguments": args}
        }));
        self.read_id(id)
    }

    fn library_new(&mut self, library: &str, src: &Path) -> serde_json::Value {
        self.call(
            "library",
            serde_json::json!({
                "action": "new",
                "library": library,
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

fn store_dir(data: &Path, id: &str, namespace: &str) -> PathBuf {
    data.join("sourcetrait")
        .join("nushell_mcp")
        .join(id)
        .join(namespace)
}

#[test]
fn default_id_is_user_env() {
    let data = tempfile::tempdir().expect("data");
    let cache = tempfile::tempdir().expect("cache");
    let src_root = tempfile::tempdir().expect("src");
    let src = src_root.path().join("mylib");
    let mut host = Host::spawn_with(&[], &[("USER", "udefault")], data.path(), cache.path());
    let resp = host.library_new("sourcetrait/mylib", &src);
    assert!(!has_error_path(&resp), "library(new) should succeed; got {resp}");
    let lib_dir = store_dir(data.path(), "udefault", "default")
        .join("libraries")
        .join("rig")
        .join("sourcetrait")
        .join("mylib");
    assert!(
        lib_dir.exists(),
        "default-id store should land under <user>/default/; expected {}",
        lib_dir.display(),
    );
}

#[test]
fn explicit_id_and_namespace_select_store() {
    let data = tempfile::tempdir().expect("data");
    let cache = tempfile::tempdir().expect("cache");
    let src_root = tempfile::tempdir().expect("src");
    let src = src_root.path().join("mylib");
    let mut host = Host::spawn_with(
        &["--id", "aid", "--namespace", "ns1"],
        &[],
        data.path(),
        cache.path(),
    );
    let resp = host.library_new("sourcetrait/mylib", &src);
    assert!(!has_error_path(&resp), "library(new) should succeed; got {resp}");
    let lib_dir = store_dir(data.path(), "aid", "ns1")
        .join("libraries")
        .join("rig")
        .join("sourcetrait")
        .join("mylib");
    assert!(
        lib_dir.exists(),
        "explicit --id/--namespace should select the store; expected {}",
        lib_dir.display(),
    );

    let info = host.call("info", serde_json::json!({}));
    let env = structured(&info);
    assert_eq!(env["id"].as_str(), Some("aid"), "got {env}");
    assert_eq!(env["namespace"].as_str(), Some("ns1"), "got {env}");
}

#[test]
fn namespaces_are_disjoint_stores() {
    let data = tempfile::tempdir().expect("data");
    let cache = tempfile::tempdir().expect("cache");
    let src_root = tempfile::tempdir().expect("src");
    let src = src_root.path().join("nslib");

    {
        let mut ns1 = Host::spawn_with(
            &["--id", "aid", "--namespace", "ns1"],
            &[],
            data.path(),
            cache.path(),
        );
        let _ = ns1.library_new("sourcetrait/nslib", &src);
        write_source(&src, "mod.nu", "export module m\n");
        write_source(&src, "m/mod.nu", "export use double\n");
        write_source(
            &src,
            "m/double/mod.nu",
            "export def main [args: record<x: int>]: nothing -> record<out: int> {\n{ out: ($args.x * 2) }\n}\n",
        );
        let committed = ns1.call("commit", serde_json::json!({"library": "sourcetrait/nslib"}));
        assert!(!has_error_path(&committed), "ns1 commit failed: {committed}");
        let called = ns1.call(
            "call",
            serde_json::json!({"namepath": "sourcetrait/nslib:m:double", "args": {"x": 4}}),
        );
        assert_eq!(
            structured(&called)["result"]["out"].as_i64(),
            Some(8),
            "ns1 call should work; got {called}",
        );
    }

    let mut ns2 = Host::spawn_with(
        &["--id", "aid", "--namespace", "ns2"],
        &[],
        data.path(),
        cache.path(),
    );
    let info = ns2.call("info", serde_json::json!({}));
    let libs = structured(&info)["libraries"]
        .as_array()
        .expect("libraries array");
    assert!(
        libs.is_empty(),
        "ns2 must not see ns1's libraries; got {libs:?}",
    );
    let called = ns2.call(
        "call",
        serde_json::json!({"namepath": "sourcetrait/nslib:m:double", "args": {"x": 4}}),
    );
    assert!(
        has_error_path(&called),
        "ns2 call into ns1's library must fail; got {called}",
    );
    assert!(store_dir(data.path(), "aid", "ns1").exists());
    assert!(store_dir(data.path(), "aid", "ns2").exists());
}

#[test]
fn worker_env_carries_store_coordinate_and_work_dir() {
    let data = tempfile::tempdir().expect("data");
    let cache = tempfile::tempdir().expect("cache");
    let wd = tempfile::tempdir().expect("workdir");
    let wd_str = wd.path().to_str().expect("workdir utf-8");
    let mut host = Host::spawn_with(
        &["--id", "envid", "--namespace", "envns", "--workdir", wd_str],
        &[],
        data.path(),
        cache.path(),
    );
    let resp = host.call(
        "run",
        serde_json::json!({
            "args_schema": {},
            "result_schema": {"id": "string", "ns": "string", "wd": "string"},
            "args": {},
            "body": "{ id: $env.EQUIP_ID, ns: $env.EQUIP_NAMESPACE, wd: $env.EQUIP_WORK_DIR }",
        }),
    );
    let env = structured(&resp);
    assert_eq!(
        env["result"]["id"].as_str(),
        Some("envid"),
        "run body should see EQUIP_ID; got {resp}",
    );
    assert_eq!(
        env["result"]["ns"].as_str(),
        Some("envns"),
        "run body should see EQUIP_NAMESPACE; got {resp}",
    );
    assert_eq!(
        env["result"]["wd"].as_str(),
        Some(wd_str),
        "run body should see the explicit EQUIP_WORK_DIR; got {resp}",
    );
    let info = host.call("info", serde_json::json!({}));
    assert_eq!(
        structured(&info)["work_dir"].as_str(),
        Some(wd_str),
        "info() should report the configured work_dir; got {info}",
    );
}

#[test]
fn workdir_tilde_expands_against_home() {
    let data = tempfile::tempdir().expect("data");
    let cache = tempfile::tempdir().expect("cache");
    let home = tempfile::tempdir().expect("home");
    let home_str = home.path().to_str().expect("home utf-8");
    let mut host = Host::spawn_with(
        &["--id", "tildeid", "--workdir", "~/wd_x"],
        &[("HOME", home_str)],
        data.path(),
        cache.path(),
    );
    let resp = host.call(
        "run",
        serde_json::json!({
            "args_schema": {},
            "result_schema": {"wd": "string"},
            "args": {},
            "body": "{ wd: $env.EQUIP_WORK_DIR }",
        }),
    );
    let expected = home.path().join("wd_x");
    assert_eq!(
        structured(&resp)["result"]["wd"].as_str(),
        expected.to_str(),
        "a ~/ workdir should expand against HOME; got {resp}",
    );
}

#[test]
fn workdir_defaults_under_home_proj_equip_id() {
    let data = tempfile::tempdir().expect("data");
    let cache = tempfile::tempdir().expect("cache");
    let home = tempfile::tempdir().expect("home");
    let home_str = home.path().to_str().expect("home utf-8");
    let mut host = Host::spawn_with(
        &["--id", "wdid"],
        &[("HOME", home_str)],
        data.path(),
        cache.path(),
    );
    let resp = host.call(
        "run",
        serde_json::json!({
            "args_schema": {},
            "result_schema": {"wd": "string"},
            "args": {},
            "body": "{ wd: $env.EQUIP_WORK_DIR }",
        }),
    );
    let expected = home.path().join("proj").join("equip").join("wdid");
    assert_eq!(
        structured(&resp)["result"]["wd"].as_str(),
        expected.to_str(),
        "an absent --workdir should default to <home>/proj/equip/<id>; got {resp}",
    );
}
