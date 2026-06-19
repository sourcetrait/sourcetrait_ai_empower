//! new (scaffold) + commit + strict-validator tests.
//!
//! `library(new)` establishes a library; `new([namepaths])` scaffolds module
//! / function skeletons into it; the (good or bad) source tree is written;
//! then `commit(name)` validates + upserts it. The strict-validator coverage
//! (reserved-`main` ban, missing-output-type, extra-export, empty-record
//! arg/result skeleton, mod.nu inline const/alias/def/let, no-root-function,
//! comments-only / multiline-signature / organizational-file acceptance, etc.)
//! is triggered through `commit()`.

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

    /// Establish a fresh library via `library(new)`.
    fn library_new(&mut self, name: &str, src: &Path) -> serde_json::Value {
        self.call(
            "library",
            serde_json::json!({
                "action": "new",
                "library": name,
                "source_dir": src.to_str().unwrap(),
            }),
        )
    }

    /// Scaffold a module/function namepath into an established library.
    fn scaffold(&mut self, namepath: &str) -> serde_json::Value {
        self.call("new", serde_json::json!({"namepaths": [namepath]}))
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

fn envelope_error(resp: &serde_json::Value) -> Option<&serde_json::Value> {
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
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.get("message")
                        .and_then(|m| m.as_str())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn structural_kinds(resp: &serde_json::Value) -> Vec<String> {
    envelope_error(resp)
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("structural"))
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.get("kind").and_then(|k| k.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Greppable JSON-serialized form of the error envelope for tests
/// that match substrings against error messages. Returns the full
/// response string when no envelope error is present.
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

fn valid_function_source(args_schema: &str, result_schema: &str, body: &str) -> String {
    // the 1-def `main` contract: main owns the body; the result schema comes
    // from the `: nothing -> R` output type.
    format!(
        "export def main [args: record<{args_schema}>]: nothing -> record<{result_schema}> {{\n{body}\n}}\n",
    )
}

#[test]
fn commit_happy_path_writes_repo_and_meta() {
    let mut host = Host::spawn();
    let src = host.source_dir("happylib");
    let _ = host.library_new("happylib", &src);
    write_source(&src, "mod.nu", "export module math\n");
    write_source(&src, "math/mod.nu", "export use ./double.nu\n");
    write_source(
        &src,
        "math/double.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );

    let resp = host.call("commit", serde_json::json!({"library": "happylib"}));
    assert!(!has_error_path(&resp), "commit should succeed; got {resp}");

    let lib = host.library_dir("happylib");
    assert!(lib.join("mod.nu").exists());
    assert!(lib.join("math").join("mod.nu").exists());
    assert!(lib.join("math").join("double.nu").exists());

    // Meta records ONLY source_path (no kind discriminant in 0.0.44+).
    let meta_text = std::fs::read_to_string(lib.join(".meta/library.json")).unwrap();
    let meta: serde_json::Value = serde_json::from_str(&meta_text).unwrap();
    assert_eq!(meta["source_path"].as_str(), Some(src.to_str().unwrap()));
    assert!(
        meta.get("kind").is_none(),
        "meta should not carry a kind field; got {meta}",
    );
}

#[test]
fn commit_rejects_mod_nu_with_syntax_error() {
    let mut host = Host::spawn();
    let src = host.source_dir("badmodlib");
    let _ = host.library_new("badmodlib", &src);
    // Unbalanced angle bracket -- syntax error in mod.nu.
    write_source(&src, "mod.nu", "export module foo\nexport\n");
    write_source(&src, "foo/mod.nu", "");
    let resp = host.call("commit", serde_json::json!({"library": "badmodlib"}));
    assert!(
        has_error_path(&resp),
        "syntax-error mod.nu should reject; got {resp}"
    );
    let msg = error_message(&resp);
    assert!(
        msg.contains("parse error"),
        "expected parse-error violation in mod.nu; got {msg:?}",
    );
}

#[test]
fn commit_rejects_mod_nu_referencing_missing_file() {
    let mut host = Host::spawn();
    let src = host.source_dir("missingreflib");
    let _ = host.library_new("missingreflib", &src);
    // export use references a file that doesn't exist in the tree.
    write_source(&src, "mod.nu", "export use ./does_not_exist.nu\n");
    let resp = host.call("commit", serde_json::json!({"library": "missingreflib"}));
    assert!(
        has_error_path(&resp),
        "missing-ref mod.nu should reject; got {resp}"
    );
    let msg = error_message(&resp);
    assert!(
        msg.contains("ModuleNotFound") || msg.contains("does_not_exist"),
        "expected ModuleNotFound for missing ref; got {msg:?}",
    );
}

#[test]
fn commit_accepts_multiline_def_signature() {
    // The `args: record<...>` sits on a different line from `export def main`.
    // The AST validator finds the positional via nu_parser's
    // Signature.required_positional inspection regardless of formatting.
    let mut host = Host::spawn();
    let src = host.source_dir("multilinelib");
    let _ = host.library_new("multilinelib", &src);
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use ./thing.nu\n");
    write_source(
        &src,
        "m/thing.nu",
        "export def main [\n    args: record<x: int>\n]: nothing -> record<out: int> {\n    { out: ($args.x * 2) }\n}\n",
    );
    let resp = host.call("commit", serde_json::json!({"library": "multilinelib"}));
    assert!(
        !has_error_path(&resp),
        "multi-line signature should pass; got {resp}"
    );
}

#[test]
fn commit_rejects_function_with_syntax_error() {
    let mut host = Host::spawn();
    let src = host.source_dir("syntaxlib");
    let _ = host.library_new("syntaxlib", &src);
    write_source(&src, "mod.nu", "");
    // Unbalanced brace -- nu_parser surfaces a parse error.
    write_source(
        &src,
        "broken.nu",
        "export def main [args: record<x: int>]: nothing -> record<out: int> {\n    { out: ($args.x * 2) \n",
    );
    let resp = host.call("commit", serde_json::json!({"library": "syntaxlib"}));
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("parse error"),
        "expected parse error violation; got {msg:?}",
    );
}

#[test]
fn commit_rejects_main_without_output_type() {
    // A call-target `main` must declare a `: nothing -> R` output type (the
    // result-schema source). A bare `[args: ...] { ... }` with no infix
    // output is rejected.
    let mut host = Host::spawn();
    let src = host.source_dir("badlib1");
    let _ = host.library_new("badlib1", &src);
    write_source(&src, "mod.nu", "");
    write_source(
        &src,
        "thing.nu",
        "export def main [args: record<x: int>] { { out: $args.x } }\n",
    );

    let resp = host.call("commit", serde_json::json!({"library": "badlib1"}));
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("output type"),
        "expected missing-output-type violation; got {msg:?}",
    );
}

#[test]
fn commit_accepts_call_target_with_helper_export() {
    // A call-target file MAY export helpers (and define private defs) beside
    // `main`; only `main` is the indexed / callable target -- no export-set
    // restriction (the_user 2026-06-18).
    let mut host = Host::spawn();
    let src = host.source_dir("helperexportlib");
    let _ = host.library_new("helperexportlib", &src);
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use ./thing.nu\n");
    write_source(
        &src,
        "m/thing.nu",
        "export def helper [n: int] { $n * 2 }\nexport def main [args: record<x: int>]: nothing -> record<out: int> { { out: $args.x } }\n",
    );

    let resp = host.call("commit", serde_json::json!({"library": "helperexportlib"}));
    assert!(
        !has_error_path(&resp),
        "a call-target may export helpers beside main; got {resp}"
    );
    // call() still targets main.
    let called = host.call(
        "call",
        serde_json::json!({"namepath": "helperexportlib:m:thing", "args": {"x": 5}}),
    );
    assert_eq!(
        called["result"]["structuredContent"]["result"]["out"].as_i64(),
        Some(5),
        "call() should run main; got {called}",
    );
}

#[test]
fn commit_rejects_main_empty_record_output() {
    // An empty `record<>` on the OUTPUT side is the unfleshed-skeleton marker
    // (the result-schema source); reject it, mirroring the arg side.
    let mut host = Host::spawn();
    let src = host.source_dir("badlib3");
    let _ = host.library_new("badlib3", &src);
    write_source(&src, "mod.nu", "");
    write_source(
        &src,
        "thing.nu",
        "export def main [args: record<x: int>]: nothing -> record<> { {} }\n",
    );

    let resp = host.call("commit", serde_json::json!({"library": "badlib3"}));
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("unfleshed skeleton") || msg.contains("real fields"),
        "expected empty-output-record violation; got {msg:?}",
    );
}

#[test]
fn commit_rejects_mod_nu_with_inline_const() {
    let mut host = Host::spawn();
    let src = host.source_dir("constmodlib");
    let _ = host.library_new("constmodlib", &src);
    write_source(&src, "mod.nu", "export module sub\nconst X = 42\n");
    write_source(&src, "sub/mod.nu", "");
    let resp = host.call("commit", serde_json::json!({"library": "constmodlib"}));
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("call to `const`") || msg.contains("may only contain"),
        "expected const-rejection violation; got {msg:?}",
    );
}

#[test]
fn commit_rejects_mod_nu_with_inline_alias() {
    let mut host = Host::spawn();
    let src = host.source_dir("aliasmodlib");
    let _ = host.library_new("aliasmodlib", &src);
    write_source(&src, "mod.nu", "export module sub\nalias foo = ls\n");
    write_source(&src, "sub/mod.nu", "");
    let resp = host.call("commit", serde_json::json!({"library": "aliasmodlib"}));
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("call to `alias`") || msg.contains("may only contain"),
        "expected alias-rejection violation; got {msg:?}",
    );
}

#[test]
fn commit_rejects_mod_nu_with_let() {
    // `let` at module body level is a PARSE ERROR per nu_parser's grammar.
    let mut host = Host::spawn();
    let src = host.source_dir("letmodlib");
    let _ = host.library_new("letmodlib", &src);
    write_source(&src, "mod.nu", "let x = 5\n");
    let resp = host.call("commit", serde_json::json!({"library": "letmodlib"}));
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("parse error") || msg.contains("Expected"),
        "expected parse error for let in mod.nu; got {msg:?}",
    );
}

#[test]
fn commit_accepts_mod_nu_with_only_comments() {
    // Empty body is legal nushell module; our convention accepts it too.
    let mut host = Host::spawn();
    let src = host.source_dir("commentedmodlib");
    let _ = host.library_new("commentedmodlib", &src);
    write_source(&src, "mod.nu", "# this library is empty\n# more comment\n");
    let resp = host.call("commit", serde_json::json!({"library": "commentedmodlib"}));
    assert!(
        !has_error_path(&resp),
        "empty mod.nu with comments should accept; got {resp}"
    );
}

#[test]
fn commit_rejects_mod_nu_with_inline_def() {
    let mut host = Host::spawn();
    let src = host.source_dir("badlib4");
    let _ = host.library_new("badlib4", &src);
    write_source(&src, "mod.nu", "export module sub\ndef helper [] { 99 }\n");
    write_source(&src, "sub/mod.nu", "");

    let resp = host.call("commit", serde_json::json!({"library": "badlib4"}));
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("mod.nu may only contain"),
        "expected mod.nu violation; got {msg:?}",
    );
}

#[test]
fn commit_aggregates_multiple_violations() {
    let mut host = Host::spawn();
    let src = host.source_dir("badlib5");
    let _ = host.library_new("badlib5", &src);
    // Three distinct violations across the tree (== the cap of 3, so all
    // surface): a bare `def` in mod.nu, an empty-args skeleton, and a main
    // lacking an output type.
    write_source(&src, "mod.nu", "export module a\ndef helper [] { 1 }\n");
    write_source(
        &src,
        "a/mod.nu",
        "export use ./skel.nu\nexport use ./noout.nu\n",
    );
    write_source(
        &src,
        "a/skel.nu",
        "export def main [args: record<>]: nothing -> record<out: int> { { out: 1 } }\n",
    );
    write_source(
        &src,
        "a/noout.nu",
        "export def main [args: record<x: int>] { { out: $args.x } }\n",
    );

    let resp = host.call("commit", serde_json::json!({"library": "badlib5"}));
    assert_eq!(
        envelope_error_kind(&resp),
        Some("library::violations"),
        "got {resp}"
    );
    let messages = structural_messages(&resp);
    // At least three distinct violation messages should appear in the
    // structural section.
    assert!(
        messages
            .iter()
            .any(|m| m.contains("mod.nu may only contain")),
        "got {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("unfleshed skeleton")),
        "got {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("output type")),
        "got {messages:?}"
    );
}

#[test]
fn commit_caps_structural_violations() {
    // structural violations cap at LINT_VIOLATION_CAP (3) and the walk
    // early-stops -- a tree with more than 3 violations rejects with exactly
    // 3 structural + structural_more = true (truthful truncation).
    let mut host = Host::spawn();
    let src = host.source_dir("caplib");
    let _ = host.library_new("caplib", &src);
    // Four bare `def`s in mod.nu -> four "mod.nu may only contain"
    // violations, more than the cap of 3.
    write_source(
        &src,
        "mod.nu",
        "export module a\ndef h1 [] { 1 }\ndef h2 [] { 2 }\ndef h3 [] { 3 }\ndef h4 [] { 4 }\n",
    );
    write_source(&src, "a/mod.nu", "");
    let resp = host.call("commit", serde_json::json!({"library": "caplib"}));
    assert_eq!(
        envelope_error_kind(&resp),
        Some("library::violations"),
        "got {resp}"
    );
    let data = envelope_error(&resp)
        .and_then(|e| e.get("data"))
        .expect("violations data");
    let structural = data
        .get("structural")
        .and_then(|s| s.as_array())
        .expect("structural array");
    assert_eq!(
        structural.len(),
        3,
        "structural should cap at 3; got {structural:?}"
    );
    assert_eq!(
        data.get("structural_more").and_then(|v| v.as_bool()),
        Some(true),
        "structural_more should be true; got {data}"
    );
}

#[test]
fn commit_rejects_root_call_target() {
    // No root functions: a call-target cannot live at the library root; it
    // must sit inside a module. A valid `main` written directly at the source
    // root is a `structure::root_function` violation.
    let mut host = Host::spawn();
    let src = host.source_dir("rootfnlib");
    let _ = host.library_new("rootfnlib", &src);
    write_source(&src, "mod.nu", "");
    write_source(
        &src,
        "thing.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let resp = host.call("commit", serde_json::json!({"library": "rootfnlib"}));
    assert_eq!(
        envelope_error_kind(&resp),
        Some("library::violations"),
        "got {resp}"
    );
    assert!(
        structural_kinds(&resp)
            .iter()
            .any(|k| k == "structure::root_function"),
        "expected structure::root_function; got {:?}",
        structural_kinds(&resp),
    );
    let messages = structural_messages(&resp);
    assert!(
        messages
            .iter()
            .any(|m| m.contains("library root") && m.contains("module")),
        "got {messages:?}"
    );
}

#[test]
fn commit_succeeds_then_check_warns_long_summary() {
    // A doc summary (here the mod.nu leading comment's first line) longer than
    // 80 chars is a WARNING, not an error: commit SUCCEEDS, and `library(check)`
    // surfaces the `lint::summary_length` warning (num_warnings > 0; ok stays
    // true since no structural error blocks).
    let mut host = Host::spawn();
    let src = host.source_dir("doclib");
    let _ = host.library_new("doclib", &src);
    let long = "x".repeat(81);
    write_source(&src, "mod.nu", &format!("# {long}\nexport module m\n"));
    write_source(&src, "m/mod.nu", "export use ./thing.nu\n");
    write_source(
        &src,
        "m/thing.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    let committed = host.call("commit", serde_json::json!({"library": "doclib"}));
    assert!(
        !has_error_path(&committed),
        "an over-long summary is advisory; commit should succeed; got {committed}"
    );

    let check = host.call(
        "library",
        serde_json::json!({
            "action": "check",
            "library": "doclib",
            "source_dir": src.to_str().unwrap(),
        }),
    );
    let summary = &check["result"]["structuredContent"]["summary"];
    assert_eq!(
        summary["ok"].as_bool(),
        Some(true),
        "warnings don't fail check; got {check}"
    );
    assert!(
        summary["num_warnings"].as_u64().unwrap_or(0) >= 1,
        "expected a warning; got {summary}"
    );
    let warn_kinds: Vec<&str> = summary["warnings"]
        .as_array()
        .expect("warnings array")
        .iter()
        .filter_map(|w| w["kind"].as_str())
        .collect();
    assert!(
        warn_kinds.iter().any(|k| *k == "lint::summary_length"),
        "expected lint::summary_length warning; got {warn_kinds:?}"
    );
}

#[test]
fn commit_accepts_short_summary() {
    // A <= 80 summary (+ details) on a node commits clean.
    let mut host = Host::spawn();
    let src = host.source_dir("okdoclib");
    let _ = host.library_new("okdoclib", &src);
    write_source(
        &src,
        "mod.nu",
        "# doubles its input\n# the math double helper module\nexport module m\n",
    );
    write_source(&src, "m/mod.nu", "export use ./thing.nu\n");
    write_source(
        &src,
        "m/thing.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let resp = host.call("commit", serde_json::json!({"library": "okdoclib"}));
    assert!(
        !has_error_path(&resp),
        "documented library should commit; got {resp}"
    );
}

#[test]
fn library_new_reestablish_duplicate_errors() {
    let mut host = Host::spawn();
    let src = host.source_dir("duplib");
    let r1 = host.library_new("duplib", &src);
    assert!(!has_error_path(&r1), "first library(new) should succeed; got {r1}");
    let r2 = host.library_new("duplib", &src);
    assert!(
        has_error_path(&r2),
        "duplicate establish should error; got {r2}"
    );
    assert_eq!(
        envelope_error_kind(&r2),
        Some("library::already_registered"),
        "got {r2}"
    );
}

#[test]
fn commit_picks_up_mutated_source() {
    let mut host = Host::spawn();
    let src = host.source_dir("livelib");
    let _ = host.library_new("livelib", &src);
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use ./thing.nu\n");
    write_source(
        &src,
        "m/thing.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let _ = host.call("commit", serde_json::json!({"library": "livelib"}));
    // Mutate the source and re-commit.
    write_source(
        &src,
        "m/thing.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x + 1000) }"),
    );
    let resp = host.call("commit", serde_json::json!({"library": "livelib"}));
    assert!(
        !has_error_path(&resp),
        "re-commit should succeed; got {resp}"
    );
    let committed =
        std::fs::read_to_string(host.library_dir("livelib").join("m").join("thing.nu")).unwrap();
    assert!(
        committed.contains("+ 1000"),
        "committed body should reflect mutated source; got {committed:?}",
    );
}

#[test]
fn commit_unknown_library_errors() {
    let mut host = Host::spawn();
    let resp = host.call("commit", serde_json::json!({"library": "ghost"}));
    assert!(has_error_path(&resp));
}

#[test]
fn committed_library_invokable_via_standalone_driver() {
    let mut host = Host::spawn();
    let src = host.source_dir("drvilib");
    let _ = host.library_new("drvilib", &src);
    write_source(&src, "mod.nu", "export module math\n");
    write_source(&src, "math/mod.nu", "export use ./double.nu\n");
    write_source(
        &src,
        "math/double.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let _ = host.call("commit", serde_json::json!({"library": "drvilib"}));
    let out = Command::new("nu")
        .env("NU_LIB_DIRS", host.libraries_dir())
        .arg("-c")
        .arg("use drvilib; drvilib math double {x: 6} | to nuon")
        .output()
        .expect("spawn nu");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "nu failed: stderr={:?}; stdout={:?}",
        String::from_utf8_lossy(&out.stderr),
        stdout,
    );
    assert!(stdout.contains("12"), "expected out: 12; got {stdout:?}",);
}

#[test]
fn commit_accepts_organizational_file() {
    // A file with no `main` sentinel is ORGANIZATIONAL: helper defs
    // + export const, unconstrained signatures, no contract. A helper file at
    // the library root is fine (only call-targets are banned there).
    let mut host = Host::spawn();
    let src = host.source_dir("orglib");
    let _ = host.library_new("orglib", &src);
    write_source(&src, "mod.nu", "export use ./util.nu\n");
    write_source(
        &src,
        "util.nu",
        "export const LIMIT = 10\nexport def helper [n: int] { $n * 2 }\n",
    );
    let resp = host.call("commit", serde_json::json!({"library": "orglib"}));
    assert!(
        !has_error_path(&resp),
        "organizational file should commit; got {resp}"
    );
}

#[test]
fn commit_accepts_mod_nu_with_export_const_and_def() {
    // mod.nu carries module-level utils/consts alongside the cascade. (leg 1)
    let mut host = Host::spawn();
    let src = host.source_dir("modutillib");
    let _ = host.library_new("modutillib", &src);
    write_source(
        &src,
        "mod.nu",
        "export const VERSION = 1\nexport def shared [] { 42 }\nexport module m\n",
    );
    write_source(&src, "m/mod.nu", "export use ./thing.nu\n");
    write_source(
        &src,
        "m/thing.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let resp = host.call("commit", serde_json::json!({"library": "modutillib"}));
    assert!(
        !has_error_path(&resp),
        "mod.nu with export const/def should commit; got {resp}"
    );
}

#[test]
fn commit_rejects_empty_record_skeleton() {
    // An empty `record<>` positional is the unfleshed-skeleton marker. (leg 1)
    let mut host = Host::spawn();
    let src = host.source_dir("skellib");
    let _ = host.library_new("skellib", &src);
    write_source(&src, "mod.nu", "");
    write_source(
        &src,
        "thing.nu",
        "export def main [args: record<>]: nothing -> record<n: int> { { n: 1 } }\n",
    );
    let resp = host.call("commit", serde_json::json!({"library": "skellib"}));
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("unfleshed skeleton") || msg.contains("real fields"),
        "expected skeleton-rejection violation; got {msg:?}",
    );
}

// ---- the reserved-terms ban ----

#[test]
fn commit_rejects_private_def_named_reserved() {
    let mut host = Host::spawn();
    let src = host.source_dir("pdeflib");
    let _ = host.library_new("pdeflib", &src);
    write_source(&src, "mod.nu", "export use ./util.nu\n");
    write_source(
        &src,
        "util.nu",
        "export const LIMIT = 5\ndef main [] { 1 }\n",
    );
    let resp = host.call("commit", serde_json::json!({"library": "pdeflib"}));
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("reserved"),
        "expected reserved-term violation; got {msg:?}"
    );
}

#[test]
fn commit_rejects_module_named_reserved() {
    let mut host = Host::spawn();
    let src = host.source_dir("modreslib");
    let _ = host.library_new("modreslib", &src);
    std::fs::create_dir_all(src.join("main")).unwrap();
    write_source(&src, "mod.nu", "export module main\n");
    write_source(&src, "main/mod.nu", "");
    let resp = host.call("commit", serde_json::json!({"library": "modreslib"}));
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("reserved"),
        "expected reserved-name violation; got {msg:?}"
    );
}

#[test]
fn commit_rejects_const_named_reserved() {
    let mut host = Host::spawn();
    let src = host.source_dir("constreslib");
    let _ = host.library_new("constreslib", &src);
    write_source(
        &src,
        "mod.nu",
        "export const main = 5\nexport module m\n",
    );
    write_source(&src, "m/mod.nu", "export use ./thing.nu\n");
    write_source(
        &src,
        "m/thing.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let resp = host.call("commit", serde_json::json!({"library": "constreslib"}));
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("reserved"),
        "expected reserved-const violation; got {msg:?}"
    );
}

#[test]
fn commit_rejects_record_key_reserved() {
    let mut host = Host::spawn();
    let src = host.source_dir("rkeylib");
    let _ = host.library_new("rkeylib", &src);
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use ./thing.nu\n");
    write_source(
        &src,
        "m/thing.nu",
        "export def main [args: record<x: int>]: nothing -> record<out: int> { { out: $args.x, main: 1 } }\n",
    );
    let resp = host.call("commit", serde_json::json!({"library": "rkeylib"}));
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("reserved"),
        "expected reserved record-key violation; got {msg:?}"
    );
}

#[test]
fn commit_rejects_cellpath_member_reserved() {
    let mut host = Host::spawn();
    let src = host.source_dir("cpathlib");
    let _ = host.library_new("cpathlib", &src);
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use ./thing.nu\n");
    write_source(
        &src,
        "m/thing.nu",
        "export def main [args: record<x: int>]: nothing -> record<out: int> { { out: $args.main } }\n",
    );
    let resp = host.call("commit", serde_json::json!({"library": "cpathlib"}));
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("reserved"),
        "expected reserved cell-path violation; got {msg:?}"
    );
}

#[test]
fn commit_rejects_param_named_reserved() {
    let mut host = Host::spawn();
    let src = host.source_dir("paramreslib");
    let _ = host.library_new("paramreslib", &src);
    write_source(&src, "mod.nu", "export use ./util.nu\n");
    write_source(&src, "util.nu", "export def helper [main: int] { $main }\n");
    let resp = host.call("commit", serde_json::json!({"library": "paramreslib"}));
    assert!(has_error_path(&resp));
    let msg = error_message(&resp);
    assert!(
        msg.contains("parameter") || msg.contains("reserved"),
        "expected reserved-param violation; got {msg:?}",
    );
}

#[test]
fn commit_accepts_reserved_as_quoted_string_value() {
    // A quoted string value keeps its quotes in the token, so an exact
    // `main` value never matches; command refs to the exports pass too.
    let mut host = Host::spawn();
    let src = host.source_dir("strvallib");
    let _ = host.library_new("strvallib", &src);
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use ./thing.nu\n");
    write_source(
        &src,
        "m/thing.nu",
        "export def main [args: record<x: int>]: nothing -> record<out: int> { let note = \"main\"; { out: $args.x } }\n",
    );
    let resp = host.call("commit", serde_json::json!({"library": "strvallib"}));
    assert!(
        !has_error_path(&resp),
        "quoted string value should not trip the ban; got {resp}"
    );
}

// ---- new() scaffold ----

#[test]
fn library_new_and_scaffold_function() {
    let mut host = Host::spawn();
    let src = host.source_dir("scaffolded");
    // Establish the library via library(new).
    let r1 = host.library_new("scaffolded", &src);
    assert!(!has_error_path(&r1), "establish should succeed; got {r1}");
    let meta_text = std::fs::read_to_string(
        host.library_dir("scaffolded")
            .join(".meta/library.json"),
    )
    .unwrap();
    assert!(
        meta_text.contains(src.to_str().unwrap()),
        "meta should record source_path; got {meta_text}",
    );
    assert!(
        src.join("mod.nu").exists(),
        "source root mod.nu should be seeded"
    );

    // Scaffold a function via new([namepath]).
    let r2 = host.scaffold("scaffolded:math:double");
    assert!(
        !has_error_path(&r2),
        "scaffold function should succeed; got {r2}"
    );
    let fn_src = std::fs::read_to_string(src.join("math").join("double.nu")).unwrap();
    assert!(
        fn_src.contains("export def main [args: record<>]: nothing -> record<>"),
        "skeleton missing the single infix-signatured main; got {fn_src:?}"
    );
    assert!(
        !fn_src.contains("export def call") && !fn_src.contains("export def resolve"),
        "skeleton should be 1-def main only; got {fn_src:?}"
    );
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
    let _ = host.library_new("guarded", &src);
    let _ = host.scaffold("guarded:m:f");
    // Scaffolding the same function again -> the leaf-guard rejects.
    let dup = host.scaffold("guarded:m:f");
    assert!(
        has_error_path(&dup),
        "scaffolding over an existing function should reject; got {dup}"
    );
}

#[test]
fn scaffold_into_unregistered_library_errors() {
    // new() scaffolds into EXISTING libraries only; it never establishes.
    let mut host = Host::spawn();
    let resp = host.scaffold("nopath:m:f");
    assert!(
        has_error_path(&resp),
        "scaffolding into an unregistered library should reject; got {resp}"
    );
    assert_eq!(
        envelope_error_kind(&resp),
        Some("library::not_registered"),
        "got {resp}"
    );
}

#[test]
fn commit_validates_and_upserts_source() {
    let mut host = Host::spawn();
    let src = host.source_dir("clib");
    let _ = host.library_new("clib", &src);
    let _ = host.scaffold("clib:math:double");
    std::fs::write(
        src.join("math").join("double.nu"),
        valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    )
    .unwrap();
    let resp = host.call("commit", serde_json::json!({"library": "clib"}));
    assert!(!has_error_path(&resp), "commit should succeed; got {resp}");
    assert!(
        host.library_dir("clib")
            .join("math")
            .join("double.nu")
            .exists(),
        "canonical should have the committed function",
    );
    let resp2 = host.call("commit", serde_json::json!({"library": "clib"}));
    assert!(
        !has_error_path(&resp2),
        "no-change commit should succeed; got {resp2}"
    );
    let sc = &resp2["result"]["structuredContent"];
    assert!(
        sc["added"].as_array().expect("added").is_empty()
            && sc["modified"].as_array().expect("modified").is_empty()
            && sc["removed"].as_array().expect("removed").is_empty(),
        "no-change commit should report nothing changed; got {sc}",
    );
}

#[test]
fn commit_rejects_unfleshed_skeleton() {
    let mut host = Host::spawn();
    let src = host.source_dir("sklib");
    let _ = host.library_new("sklib", &src);
    let _ = host.scaffold("sklib:m:raw");
    let resp = host.call("commit", serde_json::json!({"library": "sklib"}));
    assert!(
        has_error_path(&resp),
        "committing an unfleshed skeleton should reject; got {resp}"
    );
}
