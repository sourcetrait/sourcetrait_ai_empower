
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Host {
    child: Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
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
            .args(["--id", "tid", "--namespace", "default"])
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
            .join("tid")
            .join("default")
            .join("libraries")
    }

    fn canonical_dir(&self, name: &str) -> PathBuf {
        self.libraries_dir().join("rig").join("sourcetrait").join(name)
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
                "clientInfo": {"name": "library_files", "version": "0.0.1"}
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

    fn library_install(&mut self, name: &str, src: &Path) -> serde_json::Value {
        self.call(
            "library",
            serde_json::json!({"action": "install", "library": name, "source_dir": src.to_str().unwrap()}),
        )
    }

    fn commit(&mut self, name: &str) -> serde_json::Value {
        self.call("commit", serde_json::json!({"library": name}))
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
    envelope_error(resp).is_some()
}

fn envelope_error(resp: &serde_json::Value) -> Option<&serde_json::Value> {
    resp.get("result")?.get("structuredContent")?.get("error")
}

fn envelope_error_kind(resp: &serde_json::Value) -> Option<&str> {
    envelope_error(resp)?
        .get("errors")?
        .as_array()?
        .first()?
        .get("kind")?
        .as_str()
}

fn error_kinds(resp: &serde_json::Value) -> Vec<String> {
    envelope_error(resp)
        .and_then(|e| e.get("errors"))
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.get("kind").and_then(|k| k.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn has_kind(resp: &serde_json::Value, kind: &str) -> bool {
    error_kinds(resp).iter().any(|k| k == kind)
}

fn write_source(dir: &Path, rel: &str, contents: &str) {
    let target = dir.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(&target, contents).expect("write source");
}

fn chmod_x(path: &Path) {
    let mut perms = std::fs::metadata(path).expect("metadata").permissions();
    perms.set_mode(perms.mode() | 0o755);
    std::fs::set_permissions(path, perms).expect("set +x");
}

fn valid_function_source(args_schema: &str, result_schema: &str, body: &str) -> String {
    format!(
        "export def main [args: record<{args_schema}>]: nothing -> record<{result_schema}> {{\n{body}\n}}\n",
    )
}

fn author_valid_base(src: &Path) {
    write_source(src, "mod.nu", "export module m\n");
    write_source(src, "m/mod.nu", "export use double\n");
    write_source(
        src,
        "m/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
}

fn git_ls_files(repo: &Path, name: &str) -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("ls-files")
        .arg("--")
        .arg(name)
        .output()
        .expect("git ls-files");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect()
}


#[test]
fn assets_data_files_carried_recursively() {
    let mut host = Host::spawn();
    let src = host.source_dir("assetlib");
    let _ = host.library_new("assetlib", &src);
    author_valid_base(&src);
    write_source(&src, ".assets/data.json", "{\"k\": 1}\n");
    write_source(&src, ".assets/notes.txt", "hello\n");
    write_source(&src, ".assets/locale/en/main.ftl", "greeting = hi\n");
    let resp = host.commit("assetlib");
    assert!(!has_error_path(&resp), "assets commit should succeed; got {resp}");
    let canon = host.canonical_dir("assetlib");
    assert!(canon.join(".assets/data.json").exists(), "data.json carried");
    assert!(canon.join(".assets/notes.txt").exists(), "notes.txt carried");
    assert!(
        canon.join(".assets/locale/en/main.ftl").exists(),
        "nested locale carried (recursive)"
    );
}

#[test]
fn assets_denies_script_extension() {
    let mut host = Host::spawn();
    let src = host.source_dir("assetshlib");
    let _ = host.library_new("assetshlib", &src);
    author_valid_base(&src);
    write_source(&src, ".assets/run.sh", "echo hi\n");
    let resp = host.commit("assetshlib");
    assert!(
        has_kind(&resp, "library::asset_extension_denied"),
        "a .sh in .assets/ should be denied; got {:?}",
        error_kinds(&resp)
    );
}

#[test]
fn assets_dotfiles_judged_by_extension() {
    let mut host = Host::spawn();
    let src = host.source_dir("assetdotlib");
    let _ = host.library_new("assetdotlib", &src);
    author_valid_base(&src);
    write_source(&src, ".assets/.gitignore", "*.tmp\n");
    write_source(&src, ".assets/.foo", "opaque\n");
    let ok = host.commit("assetdotlib");
    assert!(
        !has_error_path(&ok),
        "extension-less dotfiles in .assets/ pass; got {ok}"
    );
    assert!(host.canonical_dir("assetdotlib").join(".assets/.gitignore").exists());
    assert!(host.canonical_dir("assetdotlib").join(".assets/.foo").exists());

    write_source(&src, ".assets/.foo.sh", "#!/bin/sh\n");
    let bad = host.commit("assetdotlib");
    assert!(
        has_kind(&bad, "library::asset_extension_denied"),
        ".foo.sh (ext sh) should be denied; got {:?}",
        error_kinds(&bad)
    );
}

#[test]
fn assets_denies_executable_bit() {
    let mut host = Host::spawn();
    let src = host.source_dir("assetxlib");
    let _ = host.library_new("assetxlib", &src);
    author_valid_base(&src);
    write_source(&src, ".assets/data.json", "{}\n");
    chmod_x(&src.join(".assets/data.json"));
    let resp = host.commit("assetxlib");
    assert!(
        has_kind(&resp, "library::asset_executable_denied"),
        "a +x .assets/ file should be denied; got {:?}",
        error_kinds(&resp)
    );
}


#[test]
fn docs_md_txt_carried() {
    let mut host = Host::spawn();
    let src = host.source_dir("doclib");
    let _ = host.library_new("doclib", &src);
    author_valid_base(&src);
    write_source(&src, ".docs/guide.md", "# guide\n");
    write_source(&src, ".docs/notes.txt", "notes\n");
    let resp = host.commit("doclib");
    assert!(!has_error_path(&resp), "docs commit should succeed; got {resp}");
    assert!(host.canonical_dir("doclib").join(".docs/guide.md").exists());
    assert!(host.canonical_dir("doclib").join(".docs/notes.txt").exists());
}

#[test]
fn docs_denies_other_extensions_and_plain_dotfile() {
    let mut host = Host::spawn();
    let src = host.source_dir("docdenylib");
    let _ = host.library_new("docdenylib", &src);
    author_valid_base(&src);
    write_source(&src, ".docs/data.json", "{}\n");
    let json = host.commit("docdenylib");
    assert!(
        has_kind(&json, "library::doc_extension_denied"),
        ".docs/data.json should be denied; got {:?}",
        error_kinds(&json)
    );

    std::fs::remove_file(src.join(".docs/data.json")).unwrap();
    write_source(&src, ".docs/.foo", "x\n");
    let foo = host.commit("docdenylib");
    assert!(
        has_kind(&foo, "library::doc_extension_denied"),
        ".docs/.foo should be denied; got {:?}",
        error_kinds(&foo)
    );
}

#[test]
fn docs_allows_gitignore() {
    let mut host = Host::spawn();
    let src = host.source_dir("docgitlib");
    let _ = host.library_new("docgitlib", &src);
    author_valid_base(&src);
    write_source(&src, ".docs/guide.md", "# guide\n");
    write_source(&src, ".docs/.gitignore", "*.bak\n");
    let resp = host.commit("docgitlib");
    assert!(
        !has_error_path(&resp),
        ".docs/.gitignore is allowlisted; got {resp}"
    );
}

#[test]
fn docs_denies_executable_bit() {
    let mut host = Host::spawn();
    let src = host.source_dir("docxlib");
    let _ = host.library_new("docxlib", &src);
    author_valid_base(&src);
    write_source(&src, ".docs/guide.md", "# guide\n");
    chmod_x(&src.join(".docs/guide.md"));
    let resp = host.commit("docxlib");
    assert!(
        has_kind(&resp, "library::doc_executable_denied"),
        "a +x .docs/ file should be denied; got {:?}",
        error_kinds(&resp)
    );
}


#[test]
fn root_sanctioned_files_carried() {
    let mut host = Host::spawn();
    let src = host.source_dir("rootlib");
    let _ = host.library_new("rootlib", &src);
    author_valid_base(&src);
    write_source(&src, "README.md", "# rootlib\n");
    write_source(&src, "LEGAL.md", "legal\n");
    write_source(&src, "LICENSE.txt", "license\n");
    write_source(&src, "LICENSE-MIT.txt", "mit\n");
    write_source(&src, "library.rig.toml", "name = \"rootlib\"\n");
    let resp = host.commit("rootlib");
    assert!(!has_error_path(&resp), "root files should commit; got {resp}");
    let canon = host.canonical_dir("rootlib");
    for f in ["README.md", "LEGAL.md", "LICENSE.txt", "LICENSE-MIT.txt", "library.rig.toml"] {
        assert!(canon.join(f).exists(), "{f} should be carried");
    }
}

#[test]
fn root_denies_unexpected_non_nu_file() {
    let mut host = Host::spawn();
    let src = host.source_dir("rootdenylib");
    let _ = host.library_new("rootdenylib", &src);
    author_valid_base(&src);
    write_source(&src, "data.json", "{}\n");
    let resp = host.commit("rootdenylib");
    assert!(
        has_kind(&resp, "library::source_extension_denied"),
        "a stray root data.json should be denied; got {:?}",
        error_kinds(&resp)
    );
}

#[test]
fn sanctioned_file_in_module_dir_denied_root_only() {
    let mut host = Host::spawn();
    let src = host.source_dir("modreadmelib");
    let _ = host.library_new("modreadmelib", &src);
    author_valid_base(&src);
    write_source(&src, "m/README.md", "# nope\n");
    let resp = host.commit("modreadmelib");
    assert!(
        has_kind(&resp, "library::source_extension_denied"),
        "a module-level README.md should be denied (root-only); got {:?}",
        error_kinds(&resp)
    );
}

#[test]
fn executable_nu_file_denied() {
    let mut host = Host::spawn();
    let src = host.source_dir("xnulib");
    let _ = host.library_new("xnulib", &src);
    author_valid_base(&src);
    chmod_x(&src.join("m/double/mod.nu"));
    let resp = host.commit("xnulib");
    assert!(
        has_kind(&resp, "library::source_executable_denied"),
        "a +x .nu file should be denied; got {:?}",
        error_kinds(&resp)
    );
}


#[test]
fn gitignore_allowed_at_root_and_module() {
    let mut host = Host::spawn();
    let src = host.source_dir("gitlib");
    let _ = host.library_new("gitlib", &src);
    author_valid_base(&src);
    write_source(&src, ".gitignore", "*.tmp\n");
    write_source(&src, "m/.gitignore", "scratch/\n");
    let resp = host.commit("gitlib");
    assert!(
        !has_error_path(&resp),
        ".gitignore is allowed at any depth; got {resp}"
    );
    let canon = host.canonical_dir("gitlib");
    assert!(canon.join(".gitignore").exists(), "root .gitignore carried");
    assert!(canon.join("m/.gitignore").exists(), "module .gitignore carried");
}

#[test]
fn gitignore_honored_at_commit_staging_only() {
    let mut host = Host::spawn();
    let src = host.source_dir("honorlib");
    let _ = host.library_new("honorlib", &src);
    author_valid_base(&src);
    write_source(&src, ".gitignore", "ignored.md\n");
    write_source(&src, ".docs/ignored.md", "secret\n");
    write_source(&src, ".docs/kept.md", "public\n");
    let resp = host.commit("honorlib");
    assert!(!has_error_path(&resp), "commit should succeed; got {resp}");

    let canon = host.canonical_dir("honorlib");
    assert!(
        canon.join(".docs/ignored.md").exists(),
        "ignored file is still copied to the canonical on disk"
    );
    let tracked = git_ls_files(&host.libraries_dir(), "rig/sourcetrait/honorlib");
    assert!(
        tracked.iter().any(|p| p.ends_with(".docs/kept.md")),
        "kept.md should be tracked; got {tracked:?}"
    );
    assert!(
        tracked.iter().any(|p| p.ends_with("honorlib/.gitignore")),
        ".gitignore should be tracked; got {tracked:?}"
    );
    assert!(
        !tracked.iter().any(|p| p.ends_with(".docs/ignored.md")),
        "the gitignored file must NOT be tracked (honored at commit); got {tracked:?}"
    );
}

#[test]
fn nested_assets_dir_is_skipped_not_carried() {
    let mut host = Host::spawn();
    let src = host.source_dir("nestlib");
    let _ = host.library_new("nestlib", &src);
    author_valid_base(&src);
    write_source(&src, "m/.assets/data.json", "{}\n");
    let resp = host.commit("nestlib");
    assert!(
        !has_error_path(&resp),
        "a nested .assets is just a skipped dotfile; commit should succeed; got {resp}"
    );
    assert!(
        !host.canonical_dir("nestlib").join("m/.assets").exists(),
        "a nested .assets must not be carried"
    );
}


#[test]
fn library_name_denylist() {
    let mut host = Host::spawn();
    for denied in ["docs", "tools", "bin", "target"] {
        let src = host.source_dir(&format!("src_{denied}"));
        let resp = host.library_new(denied, &src);
        assert_eq!(
            envelope_error_kind(&resp),
            Some("library::name_denied"),
            "library name `{denied}` should be denied; got {resp}"
        );
    }
    let src = host.source_dir("okname");
    let ok = host.library_new("okname", &src);
    assert!(!has_error_path(&ok), "a normal name should succeed; got {ok}");
}

#[test]
fn install_carries_assets_and_is_callable() {
    let mut host = Host::spawn();
    let src = host.source_dir("shipassetlib");
    author_valid_base(&src);
    write_source(&src, ".assets/locale/en/main.ftl", "k = v\n");
    write_source(&src, "README.md", "# ship\n");
    let resp = host.library_install("shipassetlib", &src);
    assert!(!has_error_path(&resp), "install should succeed; got {resp}");
    let canon = host.canonical_dir("shipassetlib");
    assert!(
        canon.join(".assets/locale/en/main.ftl").exists(),
        "install must carry .assets/ too"
    );
    assert!(canon.join("README.md").exists(), "install carries root README.md");
    let called = host.call_np("shipassetlib:m:double", serde_json::json!({"x": 21}));
    assert_eq!(
        called["result"]["structuredContent"]["result"]["out"].as_i64(),
        Some(42),
        "installed library should be callable; got {called}"
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
