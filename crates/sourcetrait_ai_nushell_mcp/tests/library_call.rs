
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

fn has_error_path(resp: &serde_json::Value) -> bool {
    resp.get("result")
        .and_then(|r| r.get("structuredContent"))
        .and_then(|sc| sc.get("error"))
        .is_some()
}

fn extract_envelope(resp: &serde_json::Value) -> Option<serde_json::Value> {
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
        "export def main [args: record<{args_schema}>]: nothing -> record<{result_schema}> {{\n{body}\n}}\n",
    )
}

#[test]
fn call_after_commit_returns_result() {
    let mut host = Host::spawn();
    let src = host.source_dir("calc");
    let _ = host.library_new("calc", &src);
    let _ = host.scaffold("calc:math:double");
    write_source(
        &src,
        "math/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let _ = host.call_tool("commit", serde_json::json!({"library": "calc"}));
    let resp = host.call_np("calc:math:double", serde_json::json!({"x": 7}));
    let env = extract_envelope(&resp).unwrap_or_else(|| panic!("call envelope; got {resp}"));
    assert_eq!(env["result"]["out"].as_i64(), Some(14));
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
fn call_after_module_commit_returns_result() {
    let mut host = Host::spawn();
    let src = host.source_dir("importable");
    let _ = host.library_new("importable", &src);
    let _ = host.scaffold("importable:util:triple");
    write_source(
        &src,
        "util/triple/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 3) }"),
    );
    let _ = host.call_tool("commit", serde_json::json!({"library": "importable"}));
    let resp = host.call_np("importable:util:triple", serde_json::json!({"x": 11}));
    let env = extract_envelope(&resp).unwrap_or_else(|| panic!("call envelope; got {resp}"));
    assert_eq!(env["result"]["out"].as_i64(), Some(33));
}

#[test]
fn call_unknown_library_errors() {
    let mut host = Host::spawn();
    let resp = host.call_np("ghost:m:noop", serde_json::json!({"noop": 0}));
    assert!(has_error_path(&resp));
}

#[test]
fn call_missing_function_errors() {
    let mut host = Host::spawn();
    let src = host.source_dir("partlib");
    let _ = host.library_new("partlib", &src);
    let resp = host.call_np("partlib:m:ghost", serde_json::json!({"noop": 0}));
    assert!(has_error_path(&resp));
}

#[test]
fn call_bad_module_path_errors() {
    let mut host = Host::spawn();
    let src = host.source_dir("safelib");
    let _ = host.library_new("safelib", &src);
    for bad in ["safelib:../etc:x", "safelib:a/../b:x", "safelib:/abs:x"] {
        let resp = host.call_np(bad, serde_json::json!({"n": 0}));
        assert!(
            has_error_path(&resp),
            "namepath {bad:?} should error; got {resp}"
        );
    }
}

#[test]
fn call_args_typecheck_failure_surfaces() {
    let mut host = Host::spawn();
    let src = host.source_dir("strictlib");
    let _ = host.library_new("strictlib", &src);
    let _ = host.scaffold("strictlib:m:needs_int");
    write_source(
        &src,
        "m/needs_int/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    let _ = host.call_tool("commit", serde_json::json!({"library": "strictlib"}));
    let resp = host.call_np("strictlib:m:needs_int", serde_json::json!({"x": "five"}));
    assert!(
        has_error_path(&resp),
        "type mismatch should surface as error; got {resp}"
    );
}

#[test]
fn inspect_returns_function_doc() {
    let mut host = Host::spawn();
    let src = host.source_dir("inspectlib");
    let _ = host.library_new("inspectlib", &src);
    let _ = host.scaffold("inspectlib:math:double");
    write_source(
        &src,
        "math/double/mod.nu",
        "# doubles its input\n#\n# returns the doubled value\nexport def main [args: record<x: int>]: nothing -> record<out: int> { { out: ($args.x * 2) } }\n",
    );
    let _ = host.call_tool("commit", serde_json::json!({"library": "inspectlib"}));
    let resp = host.call_tool(
        "inspect",
        serde_json::json!({"namepath": "inspectlib:math:double"}),
    );
    let env = extract_envelope(&resp).unwrap_or_else(|| panic!("inspect envelope; got {resp}"));
    assert_eq!(env["summary"].as_str(), Some("doubles its input"));
    assert_eq!(env["details"].as_str(), Some("returns the doubled value"));
    assert_eq!(env["library"].as_str(), Some("sourcetrait/inspectlib"));
    assert_eq!(env["module_path"].as_str(), Some("math"));
    assert_eq!(env["name"].as_str(), Some("double"));
    assert_eq!(env["args_schema"], serde_json::json!({"x": "int"}));
    assert_eq!(env["result_schema"], serde_json::json!({"out": "int"}));
}

#[test]
fn inspect_library_root_and_module() {
    let mut host = Host::spawn();
    let src = host.source_dir("inspectlib2");
    let _ = host.library_new("inspectlib2", &src);
    write_source(&src, "mod.nu", "# the inspectlib2 library\nexport module math\n");
    write_source(&src, "math/mod.nu", "# math helpers\nexport use double\n");
    write_source(
        &src,
        "math/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let _ = host.call_tool("commit", serde_json::json!({"library": "inspectlib2"}));
    let lib = host.call_tool("inspect", serde_json::json!({"namepath": "inspectlib2"}));
    assert_eq!(
        extract_envelope(&lib).unwrap()["summary"].as_str(),
        Some("the inspectlib2 library"),
    );
    let m = host.call_tool(
        "inspect",
        serde_json::json!({"namepath": "inspectlib2:math"}),
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
    let _ = host.library_new("inspectlib3", &src);
    let _ = host.scaffold("inspectlib3:m:f");
    write_source(
        &src,
        "m/f/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    let _ = host.call_tool("commit", serde_json::json!({"library": "inspectlib3"}));
    let resp = host.call_tool(
        "inspect",
        serde_json::json!({"namepath": "inspectlib3:m:f"}),
    );
    let env = extract_envelope(&resp).unwrap_or_else(|| panic!("inspect envelope; got {resp}"));
    assert_eq!(env["summary"].as_str(), Some(""));
    assert_eq!(env["details"].as_str(), Some(""));
}

#[test]
fn inspect_unknown_library_errors() {
    let mut host = Host::spawn();
    let resp = host.call_tool("inspect", serde_json::json!({"namepath": "ghost"}));
    assert!(has_error_path(&resp));
}

#[test]
fn result_record_field_shapes_preserved() {
    let mut host = Host::spawn();
    let src = host.source_dir("fidelitylib");
    let _ = host.library_new("fidelitylib", &src);
    let _ = host.scaffold("fidelitylib:m:shapes");
    write_source(
        &src,
        "m/shapes/mod.nu",
        "export def main [args: record<n: int>]: nothing -> record<p: path, d: directory, c: cell-path, g: glob> { { p: \"x\", d: \"y\", c: $.a, g: (\"z\" | into glob) } }\n",
    );
    let committed = host.call_tool("commit", serde_json::json!({"library": "fidelitylib"}));
    assert!(!has_error_path(&committed), "commit should succeed; got {committed}");
    let resp = host.call_tool(
        "inspect",
        serde_json::json!({"namepath": "fidelitylib:m:shapes"}),
    );
    let env = extract_envelope(&resp).unwrap_or_else(|| panic!("inspect envelope; got {resp}"));
    assert_eq!(
        env["result_schema"],
        serde_json::json!({"p": "path", "d": "directory", "c": "cell-path", "g": "glob"}),
        "result fields must keep path/directory/cell-path/glob verbatim; got {}",
        env["result_schema"],
    );
    assert_eq!(env["args_schema"], serde_json::json!({"n": "int"}));
}

#[test]
fn helper_file_pruned_from_info_and_not_callable() {
    let mut host = Host::spawn();
    let src = host.source_dir("helperlib");
    let _ = host.library_new("helperlib", &src);
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use ./util.nu\nexport use real\n");
    write_source(&src, "m/util.nu", "export def helper [n: int] { $n * 2 }\n");
    write_source(
        &src,
        "m/real/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x + 1) }"),
    );
    let committed = host.call_tool("commit", serde_json::json!({"library": "helperlib"}));
    assert!(!has_error_path(&committed), "commit should succeed; got {committed}");

    let info = host.call_tool("info", serde_json::json!({}));
    let libs = info["result"]["structuredContent"]["libraries"]
        .as_array()
        .expect("libraries array");
    let lib = libs
        .iter()
        .find(|l| l["name"].as_str() == Some("sourcetrait/helperlib"))
        .expect("helperlib present in info() (not dropped by the helper file)");
    let module = lib["modules"]
        .as_array()
        .expect("modules")
        .iter()
        .find(|m| m["name"].as_str() == Some("m"))
        .expect("module m present");
    let fn_names: Vec<&str> = module["functions"]
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

    let ok = host.call_np("helperlib:m:real", serde_json::json!({"x": 41}));
    let env = extract_envelope(&ok).unwrap_or_else(|| panic!("real call; got {ok}"));
    assert_eq!(env["result"]["out"].as_i64(), Some(42));

    let bad = host.call_np("helperlib:m:util", serde_json::json!({"n": 5}));
    assert!(
        has_error_path(&bad),
        "an organizational helper file must NOT be callable; got {bad}",
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
