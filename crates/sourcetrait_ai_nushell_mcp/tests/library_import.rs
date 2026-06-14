//! import_library + reimport_library + strict validator tests for 0.0.12.

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

    fn libraries_dir(&self) -> PathBuf {
        self.data_dir
            .path()
            .join("sourcetrait")
            .join("nushell_mcp")
            .join("libraries")
    }

    fn library_dir(&self, name: &str) -> PathBuf {
        self.libraries_dir().join(name)
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
                "clientInfo": {"name": "library_import", "version": "0.0.1"}
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
            "params": {"name": tool, "arguments": args}
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

fn has_error_path(resp: &serde_json::Value) -> bool {
    envelope_error(resp).is_some()
}

fn envelope_error<'a>(resp: &'a serde_json::Value) -> Option<&'a serde_json::Value> {
    resp.get("result")?.get("structuredContent")?.get("error")
}

fn envelope_error_kind(resp: &serde_json::Value) -> Option<&str> {
    envelope_error(resp)?.get("kind")?.as_str()
}

fn structural_messages(resp: &serde_json::Value) -> Vec<String> {
    envelope_error(resp)
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("structural"))
        .and_then(|s| s.as_array())
        .map(|arr| arr.iter()
            .filter_map(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_string))
            .collect())
        .unwrap_or_default()
}

/// Greppable JSON-serialized form of the error envelope for tests
/// that match substrings against error messages. Returns the full
/// response string when no envelope error is present.
fn error_message(resp: &serde_json::Value) -> String {
    envelope_error(resp).map(|e| e.to_string()).unwrap_or_else(|| resp.to_string())
}

fn write_source(dir: &Path, rel: &str, contents: &str) {
    let target = dir.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(&target, contents).expect("write source");
}

fn valid_function_source(args_schema: &str, result_schema: &str, body: &str) -> String {
    // leg 1: the call / resolve / main contract -- `body` is the raw logic
    // (now in `call`); main is the AST-locked sugar.
    format!(
        "export def call [args: record<{args_schema}>] {{\n{body}\n}}\n\nexport def resolve [args: record<{result_schema}>] {{\n    $args\n}}\n\nexport def main [args: record<{args_schema}>] {{\n    resolve (call $args)\n}}\n",
    )
}

#[test]
fn import_happy_path_writes_repo_and_meta() {
    let mut host = Host::spawn();
    let src = host.source_dir("happylib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "export module math\n");
    write_source(&src, "math/mod.nu", "export use ./double.nu\n");
    write_source(
        &src,
        "math/double.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );

    let resp = host.call(
        "import_library",
        serde_json::json!({
            "name": "happylib",
            "path": src.to_str().unwrap(),
        }),
    );
    assert!(!has_error_path(&resp), "import should succeed; got {resp}");

    let lib = host.library_dir("happylib");
    assert!(lib.join("mod.nu").exists());
    assert!(lib.join("math").join("mod.nu").exists());
    assert!(lib.join("math").join("double.nu").exists());

    // Meta records kind=imported and source_path.
    let meta_text = std::fs::read_to_string(lib.join(".nushell_mcp_meta.json")).unwrap();
    let meta: serde_json::Value = serde_json::from_str(&meta_text).unwrap();
    assert_eq!(meta["kind"].as_str(), Some("imported"));
    assert_eq!(meta["source_path"].as_str(), Some(src.to_str().unwrap()));
}

#[test]
fn import_rejects_mod_nu_with_syntax_error() {
    let mut host = Host::spawn();
    let src = host.source_dir("badmodlib");
    std::fs::create_dir_all(&src).unwrap();
    // Unbalanced angle bracket -- syntax error in mod.nu.
    write_source(&src, "mod.nu", "export module foo\nexport\n");
    write_source(&src, "foo/mod.nu", "");
    let resp = host.call(
        "import_library",
        serde_json::json!({
            "name": "badmodlib",
            "path": src.to_str().unwrap(),
        }),
    );
    assert!(has_error_path(&resp), "syntax-error mod.nu should reject; got {resp}");
    let msg = error_message(&resp);
    assert!(
        msg.contains("parse error"),
        "expected parse-error violation in mod.nu; got {msg:?}",
    );
}

#[test]
fn import_rejects_mod_nu_referencing_missing_file() {
    let mut host = Host::spawn();
    let src = host.source_dir("missingreflib");
    std::fs::create_dir_all(&src).unwrap();
    // export use references a file that doesn't exist in the tree.
    write_source(&src, "mod.nu", "export use ./does_not_exist.nu\n");
    let resp = host.call(
        "import_library",
        serde_json::json!({
            "name": "missingreflib",
            "path": src.to_str().unwrap(),
        }),
    );
    assert!(has_error_path(&resp), "missing-ref mod.nu should reject; got {resp}");
    let msg = error_message(&resp);
    assert!(
        msg.contains("ModuleNotFound") || msg.contains("does_not_exist"),
        "expected ModuleNotFound for missing ref; got {msg:?}",
    );
}

#[test]
fn import_accepts_multiline_def_signature() {
    // Text-based validator would have FAILED this -- the `args: record<...>`
    // sits on a different line from `export def main`, so the line-level
    // signature check missed it. AST validator finds the positional via
    // nu_parser's Signature.required_positional inspection regardless of
    // formatting.
    let mut host = Host::spawn();
    let src = host.source_dir("multilinelib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "");
    write_source(
        &src,
        "thing.nu",
        "export def call [\n    args: record<x: int>\n] {\n    { out: ($args.x * 2) }\n}\n\nexport def resolve [\n    args: record<out: int>\n] {\n    $args\n}\n\nexport def main [args: record<x: int>] {\n    resolve (call $args)\n}\n",
    );
    let resp = host.call(
        "import_library",
        serde_json::json!({
            "name": "multilinelib",
            "path": src.to_str().unwrap(),
        }),
    );
    assert!(!has_error_path(&resp), "multi-line signature should pass; got {resp}");
}

#[test]
fn import_rejects_function_with_syntax_error() {
    let mut host = Host::spawn();
    let src = host.source_dir("syntaxlib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "");
    // Unbalanced brace -- nu_parser surfaces a parse error.
    write_source(
        &src,
        "broken.nu",
        "export def main [args: record<x: int>] {\n    { out: ($args.x * 2) \nexport def resolve [args: record<out: int>] { $args }\n",
    );
    let resp = host.call(
        "import_library",
        serde_json::json!({
            "name": "syntaxlib",
            "path": src.to_str().unwrap(),
        }),
    );
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("parse error"),
        "expected parse error violation; got {msg:?}",
    );
}

#[test]
fn import_rejects_call_target_missing_resolve() {
    // A file exporting `call` is a call-target; it must complete the
    // contract. Missing `resolve` -> reject. (leg 1)
    let mut host = Host::spawn();
    let src = host.source_dir("badlib1");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "");
    write_source(
        &src,
        "thing.nu",
        "export def call [args: record<x: int>] { { out: $args.x } }\nexport def main [args: record<x: int>] { resolve (call $args) }\n",
    );

    let resp = host.call(
        "import_library",
        serde_json::json!({
            "name": "badlib1",
            "path": src.to_str().unwrap(),
        }),
    );
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("must export `resolve`"),
        "expected missing-resolve violation; got {msg:?}",
    );
}

#[test]
fn import_rejects_call_target_extra_export() {
    let mut host = Host::spawn();
    let src = host.source_dir("badlib2");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "");
    write_source(
        &src,
        "thing.nu",
        "export def call [args: record<x: int>] { { out: $args.x } }\nexport def resolve [args: record<out: int>] { $args }\nexport def main [args: record<x: int>] { resolve (call $args) }\nexport def helper [args: record<x: int>] { $args.x }\n",
    );

    let resp = host.call(
        "import_library",
        serde_json::json!({
            "name": "badlib2",
            "path": src.to_str().unwrap(),
        }),
    );
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("exports only `call`, `resolve`, and `main`"),
        "expected extra-export violation; got {msg:?}",
    );
}

#[test]
fn import_rejects_non_passthrough_resolve_body() {
    let mut host = Host::spawn();
    let src = host.source_dir("badlib3");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "");
    write_source(
        &src,
        "thing.nu",
        "export def call [args: record<x: int>] { { out: $args.x } }\nexport def resolve [args: record<out: int>] { print $args; $args }\nexport def main [args: record<x: int>] { resolve (call $args) }\n",
    );

    let resp = host.call(
        "import_library",
        serde_json::json!({
            "name": "badlib3",
            "path": src.to_str().unwrap(),
        }),
    );
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("must be exactly `$args`"),
        "expected resolve-body violation; got {msg:?}",
    );
}

#[test]
fn import_rejects_mod_nu_with_inline_const() {
    let mut host = Host::spawn();
    let src = host.source_dir("constmodlib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "export module sub\nconst X = 42\n");
    write_source(&src, "sub/mod.nu", "");
    let resp = host.call(
        "import_library",
        serde_json::json!({
            "name": "constmodlib",
            "path": src.to_str().unwrap(),
        }),
    );
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("call to `const`") || msg.contains("may only contain"),
        "expected const-rejection violation; got {msg:?}",
    );
}

#[test]
fn import_rejects_mod_nu_with_inline_alias() {
    let mut host = Host::spawn();
    let src = host.source_dir("aliasmodlib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "export module sub\nalias foo = ls\n");
    write_source(&src, "sub/mod.nu", "");
    let resp = host.call(
        "import_library",
        serde_json::json!({
            "name": "aliasmodlib",
            "path": src.to_str().unwrap(),
        }),
    );
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("call to `alias`") || msg.contains("may only contain"),
        "expected alias-rejection violation; got {msg:?}",
    );
}

#[test]
fn import_rejects_mod_nu_with_let() {
    // 0.0.16: `let` at module body level is a PARSE ERROR per
    // nu_parser's grammar (not even on the allowed-keyword list).
    // The AST validator surfaces it via parse_errors.
    let mut host = Host::spawn();
    let src = host.source_dir("letmodlib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "let x = 5\n");
    let resp = host.call(
        "import_library",
        serde_json::json!({
            "name": "letmodlib",
            "path": src.to_str().unwrap(),
        }),
    );
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("parse error") || msg.contains("Expected"),
        "expected parse error for let in mod.nu; got {msg:?}",
    );
}

#[test]
fn import_accepts_mod_nu_with_only_comments() {
    // Empty body is legal nushell module; our convention accepts it too.
    let mut host = Host::spawn();
    let src = host.source_dir("commentedmodlib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "# this library is empty\n# more comment\n");
    let resp = host.call(
        "import_library",
        serde_json::json!({
            "name": "commentedmodlib",
            "path": src.to_str().unwrap(),
        }),
    );
    assert!(!has_error_path(&resp), "empty mod.nu with comments should accept; got {resp}");
}

#[test]
fn import_rejects_mod_nu_with_inline_def() {
    let mut host = Host::spawn();
    let src = host.source_dir("badlib4");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "export module sub\ndef helper [] { 99 }\n");
    write_source(&src, "sub/mod.nu", "");

    let resp = host.call(
        "import_library",
        serde_json::json!({
            "name": "badlib4",
            "path": src.to_str().unwrap(),
        }),
    );
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("mod.nu may only contain"),
        "expected mod.nu violation; got {msg:?}",
    );
}

#[test]
fn import_aggregates_multiple_violations() {
    let mut host = Host::spawn();
    let src = host.source_dir("badlib5");
    std::fs::create_dir_all(&src).unwrap();
    // leg 1: a bare `def` in mod.nu (only export forms allowed), a
    // call-target missing `call`, and a call-target whose `main` body is
    // wrong -- three distinct violations across the tree.
    write_source(&src, "mod.nu", "export module a\ndef helper [] { 1 }\n");
    write_source(&src, "a/mod.nu", "");
    write_source(&src, "a/no_call.nu", "export def resolve [args: record<x: int>] { $args }\nexport def main [args: record<x: int>] { resolve (call $args) }\n");
    write_source(&src, "a/bad_main.nu", "export def call [args: record<x: int>] { { out: $args.x } }\nexport def resolve [args: record<out: int>] { $args }\nexport def main [args: record<x: int>] { call $args }\n");

    let resp = host.call(
        "import_library",
        serde_json::json!({
            "name": "badlib5",
            "path": src.to_str().unwrap(),
        }),
    );
    assert_eq!(envelope_error_kind(&resp), Some("library::violations"), "got {resp}");
    let messages = structural_messages(&resp);
    // At least three distinct violation messages should appear in the
    // structural section.
    assert!(messages.iter().any(|m| m.contains("mod.nu may only contain")),
        "got {messages:?}");
    assert!(messages.iter().any(|m| m.contains("must export `call`")),
        "got {messages:?}");
    assert!(messages.iter().any(|m| m.contains("main's body must be exactly")),
        "got {messages:?}");
}

#[test]
fn import_duplicate_name_errors() {
    let mut host = Host::spawn();
    let src = host.source_dir("duplib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "");
    let r1 = host.call(
        "import_library",
        serde_json::json!({
            "name": "duplib",
            "path": src.to_str().unwrap(),
        }),
    );
    assert!(!has_error_path(&r1), "first import should succeed; got {r1}");
    let r2 = host.call(
        "import_library",
        serde_json::json!({
            "name": "duplib",
            "path": src.to_str().unwrap(),
        }),
    );
    assert!(has_error_path(&r2), "duplicate import should error; got {r2}");
}

#[test]
fn reimport_picks_up_changes() {
    let mut host = Host::spawn();
    let src = host.source_dir("livelib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "");
    write_source(
        &src,
        "thing.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let _ = host.call(
        "import_library",
        serde_json::json!({
            "name": "livelib",
            "path": src.to_str().unwrap(),
        }),
    );
    // Mutate the source and reimport.
    write_source(
        &src,
        "thing.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x + 1000) }"),
    );
    let resp = host.call("reimport_library", serde_json::json!({"name": "livelib"}));
    assert!(!has_error_path(&resp), "reimport should succeed; got {resp}");
    let imported = std::fs::read_to_string(host.library_dir("livelib").join("thing.nu")).unwrap();
    assert!(
        imported.contains("+ 1000"),
        "imported body should reflect mutated source; got {imported:?}",
    );
}

#[test]
fn reimport_rejects_when_kind_is_registered() {
    let mut host = Host::spawn();
    let mirror = host.source_dir("regmirror");
    let _ = host.call(
        "register_library",
        serde_json::json!({
            "name": "registered_lib",
            "path": mirror.to_str().unwrap(),
        }),
    );
    let resp = host.call(
        "reimport_library",
        serde_json::json!({"name": "registered_lib"}),
    );
    assert_eq!(envelope_error_kind(&resp), Some("library::wrong_kind"),
        "expected library::wrong_kind; got {resp}");
}

#[test]
fn reimport_unknown_library_errors() {
    let mut host = Host::spawn();
    let resp = host.call(
        "reimport_library",
        serde_json::json!({"name": "ghost"}),
    );
    assert!(has_error_path(&resp));
}

#[test]
fn imported_library_invokable_via_standalone_driver() {
    let mut host = Host::spawn();
    let src = host.source_dir("drvilib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "export module math\n");
    write_source(&src, "math/mod.nu", "export use ./double.nu\n");
    write_source(
        &src,
        "math/double.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let _ = host.call(
        "import_library",
        serde_json::json!({
            "name": "drvilib",
            "path": src.to_str().unwrap(),
        }),
    );
    let out = Command::new("nu")
        .env("NU_LIB_DIRS", host.libraries_dir())
        .arg("-c")
        .arg("use drvilib; drvilib math double resolve (drvilib math double {x: 6}) | to nuon")
        .output()
        .expect("spawn nu");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "nu failed: stderr={:?}; stdout={:?}",
        String::from_utf8_lossy(&out.stderr),
        stdout,
    );
    assert!(
        stdout.contains("12"),
        "expected out: 12; got {stdout:?}",
    );
}

#[test]
fn import_accepts_organizational_file() {
    // A file with no call/resolve sentinel is ORGANIZATIONAL: helper defs
    // + export const, unconstrained signatures, no contract. (leg 1)
    let mut host = Host::spawn();
    let src = host.source_dir("orglib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "export use ./util.nu\n");
    write_source(
        &src,
        "util.nu",
        "export const LIMIT = 10\nexport def helper [n: int] { $n * 2 }\n",
    );
    let resp = host.call(
        "import_library",
        serde_json::json!({"name": "orglib", "path": src.to_str().unwrap()}),
    );
    assert!(!has_error_path(&resp), "organizational file should import; got {resp}");
}

#[test]
fn import_accepts_mod_nu_with_export_const_and_def() {
    // mod.nu carries module-level utils/consts alongside the cascade. (leg 1)
    let mut host = Host::spawn();
    let src = host.source_dir("modutillib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(
        &src,
        "mod.nu",
        "export const VERSION = 1\nexport def shared [] { 42 }\nexport use ./thing.nu\n",
    );
    write_source(
        &src,
        "thing.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let resp = host.call(
        "import_library",
        serde_json::json!({"name": "modutillib", "path": src.to_str().unwrap()}),
    );
    assert!(!has_error_path(&resp), "mod.nu with export const/def should import; got {resp}");
}

#[test]
fn import_rejects_empty_record_skeleton() {
    // An empty `record<>` positional is the unfleshed-skeleton marker. (leg 1)
    let mut host = Host::spawn();
    let src = host.source_dir("skellib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "");
    write_source(
        &src,
        "thing.nu",
        "export def call [args: record<>] { {} }\nexport def resolve [args: record<n: int>] { $args }\nexport def main [args: record<>] { resolve (call $args) }\n",
    );
    let resp = host.call(
        "import_library",
        serde_json::json!({"name": "skellib", "path": src.to_str().unwrap()}),
    );
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("unfleshed skeleton") || msg.contains("real fields"),
        "expected skeleton-rejection violation; got {msg:?}",
    );
}

// ---- leg 1b: the reserved-terms ban ----

#[test]
fn import_rejects_private_def_named_reserved() {
    let mut host = Host::spawn();
    let src = host.source_dir("pdeflib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "export use ./util.nu\n");
    write_source(&src, "util.nu", "export const LIMIT = 5\ndef call [] { 1 }\n");
    let resp = host.call(
        "import_library",
        serde_json::json!({"name": "pdeflib", "path": src.to_str().unwrap()}),
    );
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(msg.contains("reserved"), "expected reserved-term violation; got {msg:?}");
}

#[test]
fn import_rejects_module_named_reserved() {
    let mut host = Host::spawn();
    let src = host.source_dir("modreslib");
    std::fs::create_dir_all(src.join("resolve")).unwrap();
    write_source(&src, "mod.nu", "export module resolve\n");
    write_source(&src, "resolve/mod.nu", "");
    let resp = host.call(
        "import_library",
        serde_json::json!({"name": "modreslib", "path": src.to_str().unwrap()}),
    );
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(msg.contains("reserved"), "expected reserved-name violation; got {msg:?}");
}

#[test]
fn import_rejects_const_named_reserved() {
    let mut host = Host::spawn();
    let src = host.source_dir("constreslib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "export const call = 5\nexport use ./thing.nu\n");
    write_source(
        &src,
        "thing.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let resp = host.call(
        "import_library",
        serde_json::json!({"name": "constreslib", "path": src.to_str().unwrap()}),
    );
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(msg.contains("reserved"), "expected reserved-const violation; got {msg:?}");
}

#[test]
fn import_rejects_record_key_reserved() {
    let mut host = Host::spawn();
    let src = host.source_dir("rkeylib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "");
    write_source(
        &src,
        "thing.nu",
        "export def call [args: record<x: int>] { { call: $args.x } }\nexport def resolve [args: record<n: int>] { $args }\nexport def main [args: record<x: int>] { resolve (call $args) }\n",
    );
    let resp = host.call(
        "import_library",
        serde_json::json!({"name": "rkeylib", "path": src.to_str().unwrap()}),
    );
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(msg.contains("reserved"), "expected reserved record-key violation; got {msg:?}");
}

#[test]
fn import_rejects_cellpath_member_reserved() {
    let mut host = Host::spawn();
    let src = host.source_dir("cpathlib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "");
    write_source(
        &src,
        "thing.nu",
        "export def call [args: record<x: int>] { { out: $args.resolve } }\nexport def resolve [args: record<out: int>] { $args }\nexport def main [args: record<x: int>] { resolve (call $args) }\n",
    );
    let resp = host.call(
        "import_library",
        serde_json::json!({"name": "cpathlib", "path": src.to_str().unwrap()}),
    );
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(msg.contains("reserved"), "expected reserved cell-path violation; got {msg:?}");
}

#[test]
fn import_rejects_param_named_reserved() {
    let mut host = Host::spawn();
    let src = host.source_dir("paramreslib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "export use ./util.nu\n");
    write_source(&src, "util.nu", "export def helper [call: int] { $call }\n");
    let resp = host.call(
        "import_library",
        serde_json::json!({"name": "paramreslib", "path": src.to_str().unwrap()}),
    );
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("parameter") || msg.contains("reserved"),
        "expected reserved-param violation; got {msg:?}",
    );
}

#[test]
fn import_accepts_reserved_as_quoted_string_value() {
    // A quoted string value keeps its quotes in the token, so an exact
    // `resolve` value never matches; command refs to the exports pass too.
    let mut host = Host::spawn();
    let src = host.source_dir("strvallib");
    std::fs::create_dir_all(&src).unwrap();
    write_source(&src, "mod.nu", "");
    write_source(
        &src,
        "thing.nu",
        "export def call [args: record<x: int>] { let note = \"resolve\"; { out: $args.x } }\nexport def resolve [args: record<out: int>] { $args }\nexport def main [args: record<x: int>] { resolve (call $args) }\n",
    );
    let resp = host.call(
        "import_library",
        serde_json::json!({"name": "strvallib", "path": src.to_str().unwrap()}),
    );
    assert!(!has_error_path(&resp), "quoted string value should not trip the ban; got {resp}");
}

// ---- leg 3: new() scaffold ----

#[test]
fn new_establishes_library_and_scaffolds_function() {
    let mut host = Host::spawn();
    let src = host.source_dir("scaffolded");
    // Establish the library (first new(); source_path required).
    let r1 = host.call(
        "new",
        serde_json::json!({"library": "scaffolded", "source_path": src.to_str().unwrap()}),
    );
    assert!(!has_error_path(&r1), "establish should succeed; got {r1}");
    let meta_text = std::fs::read_to_string(
        host.library_dir("scaffolded").join(".nushell_mcp_meta.json"),
    )
    .unwrap();
    assert!(
        meta_text.contains(src.to_str().unwrap()),
        "meta should record source_path; got {meta_text}",
    );
    assert!(src.join("mod.nu").exists(), "source root mod.nu should be seeded");

    // Scaffold a function (later call; source_path omitted).
    let r2 = host.call(
        "new",
        serde_json::json!({"library": "scaffolded", "module_path": "math", "name": "double"}),
    );
    assert!(!has_error_path(&r2), "scaffold function should succeed; got {r2}");
    let fn_src = std::fs::read_to_string(src.join("math").join("double.nu")).unwrap();
    assert!(fn_src.contains("export def call"), "skeleton missing call; got {fn_src:?}");
    assert!(fn_src.contains("export def resolve"), "skeleton missing resolve");
    assert!(fn_src.contains("export def main"), "skeleton missing main");
    assert!(fn_src.contains("resolve (call $args)"), "skeleton main body wrong");
    // Additive cascade wiring (NOT regenerate).
    let math_mod = std::fs::read_to_string(src.join("math").join("mod.nu")).unwrap();
    assert!(
        math_mod.contains("export use ./double.nu"),
        "math mod.nu should wire double; got {math_mod:?}",
    );
    let root_mod = std::fs::read_to_string(src.join("mod.nu")).unwrap();
    assert!(
        root_mod.contains("export module math"),
        "root mod.nu should wire math; got {root_mod:?}",
    );
}

#[test]
fn new_leaf_guard_refuses_existing_function() {
    let mut host = Host::spawn();
    let src = host.source_dir("guarded");
    let _ = host.call(
        "new",
        serde_json::json!({"library": "guarded", "source_path": src.to_str().unwrap()}),
    );
    let _ = host.call(
        "new",
        serde_json::json!({"library": "guarded", "module_path": "m", "name": "f"}),
    );
    // Scaffolding the same function again -> the leaf-guard rejects.
    let dup = host.call(
        "new",
        serde_json::json!({"library": "guarded", "module_path": "m", "name": "f"}),
    );
    assert!(has_error_path(&dup), "scaffolding over an existing function should reject; got {dup}");
}

#[test]
fn new_requires_source_path_on_establish() {
    let mut host = Host::spawn();
    // First new() for a name with no source_path -> rejected.
    let resp = host.call("new", serde_json::json!({"library": "nopath"}));
    assert!(has_error_path(&resp), "establishing new() without source_path should reject; got {resp}");
}

#[test]
fn commit_validates_and_upserts_source() {
    let mut host = Host::spawn();
    let src = host.source_dir("clib");
    let _ = host.call("new", serde_json::json!({"library": "clib", "source_path": src.to_str().unwrap()}));
    let _ = host.call("new", serde_json::json!({"library": "clib", "module_path": "math", "name": "double"}));
    std::fs::write(
        src.join("math").join("double.nu"),
        valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    )
    .unwrap();
    let resp = host.call("commit", serde_json::json!({"library": "clib"}));
    assert!(!has_error_path(&resp), "commit should succeed; got {resp}");
    assert!(
        host.library_dir("clib").join("math").join("double.nu").exists(),
        "canonical should have the committed function",
    );
    let resp2 = host.call("commit", serde_json::json!({"library": "clib"}));
    assert!(!has_error_path(&resp2), "no-change commit should succeed; got {resp2}");
    let changed = resp2["result"]["structuredContent"]["changed"].as_array().expect("changed array");
    assert!(changed.is_empty(), "no-change commit should report no changes; got {changed:?}");
}

#[test]
fn commit_rejects_unfleshed_skeleton() {
    let mut host = Host::spawn();
    let src = host.source_dir("sklib");
    let _ = host.call("new", serde_json::json!({"library": "sklib", "source_path": src.to_str().unwrap()}));
    let _ = host.call("new", serde_json::json!({"library": "sklib", "module_path": "", "name": "raw"}));
    let resp = host.call("commit", serde_json::json!({"library": "sklib"}));
    assert!(has_error_path(&resp), "committing an unfleshed skeleton should reject; got {resp}");
}
