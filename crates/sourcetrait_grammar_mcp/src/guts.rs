//! In-process test-support harness (per the test-only-`pub` -> `crate::guts`
//! convention). `TestServer` wraps a `NuSh` + a tokio runtime and drives the
//! real tool handlers in-process, returning each tool's success/error envelope
//! as a serde_json Value - the same shape the wire carries - so integration
//! tests exercise the handlers without spawning the binary.
//!
//! `CONFIG` (OnceLock) + `BASE_DIRS` (LazyLock) are process-global, so every
//! `TestServer` in a test binary shares ONE store under a per-binary temp XDG
//! root. Tests use unique library names; a test needing a pristine store lives
//! in its own binary (its own process) or the system-test crate.

use crate::*;

use std::sync::Once;

static TEST_CONFIG: Once = Once::new();

/// Point the process-global XDG roots at a per-binary temp dir and seed CONFIG,
/// once, before any `BASE_DIRS` access.
fn ensure_test_config() {
    TEST_CONFIG.call_once(|| {
        let root = std::env::temp_dir().join(format!("grammar_inproc_{}", process::id()));
        let _ = fs::create_dir_all(root.join("data"));
        let _ = fs::create_dir_all(root.join("cache"));
        // SAFETY: the Once serializes this, and it runs before the first
        // BASE_DIRS read (BASE_DIRS is only touched inside ensure_substrate /
        // the cache paths, all reached through a TestServer after this).
        unsafe {
            std::env::set_var("XDG_DATA_HOME", root.join("data"));
            std::env::set_var("XDG_CACHE_HOME", root.join("cache"));
        }
        let _ = CONFIG.set(Config {
            id: "test".to_string(),
            namespace: "default".to_string(),
            work_dir: root.join("work"),
            deny: DenySet::default(),
        });
    });
}

/// An in-process handle to the grammar tool surface. Each method drives the real
/// handler and returns its envelope (the `{ result, nonce }` success shape, or
/// the `{ error: { errors, warnings, nonce? } }` error shape) as a JSON value.
pub struct TestServer {
    rt: tokio::runtime::Runtime,
    nush: NuSh,
}

impl Default for TestServer {
    fn default() -> Self {
        Self::new()
    }
}

impl TestServer {
    pub fn new() -> Self {
        ensure_test_config();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let locks = rt.block_on(ensure_substrate()).expect("ensure_substrate");
        let nush = NuSh::new(
            Arc::new(NonceGen::new()),
            locks,
            Arc::new(ParseEngine::new_full()),
        );
        Self { rt, nush }
    }

    fn obj(v: json::Value) -> mcp::JsonObject {
        match v {
            json::Value::Object(m) => m,
            json::Value::Null => mcp::JsonObject::new(),
            other => panic!("expected a JSON object, got {other}"),
        }
    }

    fn envelope(r: Result<mcp::CallToolResult, mcp::ErrorData>) -> json::Value {
        match r {
            Ok(ctr) => ctr.structured_content.unwrap_or(json::Value::Null),
            Err(e) => panic!("unexpected rmcp-boundary error: {e:?}"),
        }
    }

    pub fn run(
        &self,
        args_schema: json::Value,
        result_schema: json::Value,
        args: json::Value,
        body: &str,
    ) -> json::Value {
        self.run_timeout(args_schema, result_schema, args, body, None)
    }

    pub fn run_timeout(
        &self,
        args_schema: json::Value,
        result_schema: json::Value,
        args: json::Value,
        body: &str,
        timeout_ms: Option<u64>,
    ) -> json::Value {
        let p = RunParams {
            args_schema: Self::obj(args_schema),
            result_schema: Self::obj(result_schema),
            args: Self::obj(args),
            body: body.to_string(),
            timeout_ms,
        };
        Self::envelope(self.rt.block_on(self.nush.run(mcp::Parameters(p))))
    }

    pub fn interact(
        &self,
        args_schema: json::Value,
        result_schema: json::Value,
        args: json::Value,
        body: &str,
    ) -> json::Value {
        let p = RunParams {
            args_schema: Self::obj(args_schema),
            result_schema: Self::obj(result_schema),
            args: Self::obj(args),
            body: body.to_string(),
            timeout_ms: None,
        };
        Self::envelope(self.rt.block_on(self.nush.interact(mcp::Parameters(p))))
    }

    pub fn rerun(&self, nonce: &str, args: json::Value) -> json::Value {
        self.rerun_timeout(nonce, args, None)
    }

    pub fn rerun_timeout(&self, nonce: &str, args: json::Value, timeout_ms: Option<u64>) -> json::Value {
        let p = RerunParams {
            nonce: nonce.to_string(),
            args: Self::obj(args),
            timeout_ms,
        };
        Self::envelope(self.rt.block_on(self.nush.rerun(mcp::Parameters(p))))
    }

    pub fn call(&self, namepath: &str, args: json::Value) -> json::Value {
        let p = CallParams {
            namepath: namepath.to_string(),
            args: Self::obj(args),
            timeout_ms: None,
        };
        Self::envelope(self.rt.block_on(self.nush.call(mcp::Parameters(p))))
    }

    pub fn commit(&self, library: &str) -> json::Value {
        let p = CommitParams {
            library: library.to_string(),
        };
        Self::envelope(self.rt.block_on(self.nush.commit(mcp::Parameters(p))))
    }

    pub fn library(&self, action: &str, library: &str, source_dir: &str) -> json::Value {
        let p = LibraryParams {
            action: action.to_string(),
            library: library.to_string(),
            source_dir: source_dir.to_string(),
        };
        Self::envelope(self.rt.block_on(self.nush.library(mcp::Parameters(p))))
    }

    pub fn scaffold(&self, namepaths: &[&str]) -> json::Value {
        let p = NewParams {
            namepaths: namepaths.iter().map(|s| s.to_string()).collect(),
        };
        Self::envelope(self.rt.block_on(self.nush.scaffold(mcp::Parameters(p))))
    }

    pub fn inspect(&self, namepath: &str) -> json::Value {
        let p = InspectParams {
            namepath: namepath.to_string(),
        };
        Self::envelope(self.rt.block_on(self.nush.inspect(mcp::Parameters(p))))
    }

    pub fn info(&self) -> json::Value {
        Self::envelope(self.rt.block_on(self.nush.info(mcp::Parameters(InfoParams {}))))
    }

    pub fn learn(&self, harness_dir: &str) -> json::Value {
        let p = LearnParams {
            harness_dir: harness_dir.to_string(),
        };
        Self::envelope(self.rt.block_on(self.nush.learn(mcp::Parameters(p))))
    }

    pub fn processes(&self) -> json::Value {
        Self::envelope(
            self.rt
                .block_on(self.nush.processes(mcp::Parameters(ProcessesParams {}))),
        )
    }

    /// `kill` is a no-return tool: the envelope is JSON null on success.
    pub fn kill(&self, nonce: &str) -> json::Value {
        let p = KillParams {
            nonce: nonce.to_string(),
        };
        Self::envelope(self.rt.block_on(self.nush.kill(mcp::Parameters(p))))
    }

    /// The in-process store's libraries git repo dir (the `(test, default)`
    /// coordinate under the per-binary temp XDG data root), for tests that
    /// inspect on-disk store artifacts (git-tracked paths, the canonical tree).
    pub fn libraries_dir(&self) -> std::path::PathBuf {
        crate::libraries_dir()
    }

    /// The canonical committed dir for a library by its compound `author/name`
    /// (under `libraries/rig/`), for on-disk carried-file / meta assertions.
    pub fn library_dir(&self, name: &str) -> std::path::PathBuf {
        crate::libraries_dir().join("rig").join(name)
    }
}

// ---- envelope readers (shared by the in-process integration tests) ----

/// True when the envelope is the error shape (`{ error: ... }`).
pub fn has_error(env: &json::Value) -> bool {
    env.get("error").is_some()
}

/// The first error diagnostic's `kind`, if any.
pub fn error_kind(env: &json::Value) -> Option<&str> {
    env.get("error")?
        .get("errors")?
        .as_array()?
        .first()?
        .get("kind")?
        .as_str()
}

/// Every error/warning diagnostic `kind` in the envelope.
pub fn error_kinds(env: &json::Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(err) = env.get("error") {
        for bucket in ["errors", "warnings"] {
            if let Some(arr) = err.get(bucket).and_then(|v| v.as_array()) {
                for d in arr {
                    if let Some(k) = d.get("kind").and_then(|k| k.as_str()) {
                        out.push(k.to_string());
                    }
                }
            }
        }
    }
    out
}

pub fn has_kind(env: &json::Value, kind: &str) -> bool {
    error_kinds(env).iter().any(|k| k == kind)
}

/// Every ERROR-bucket diagnostic `message` in the envelope (warnings excluded).
pub fn error_messages(env: &json::Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(arr) = env
        .get("error")
        .and_then(|e| e.get("errors"))
        .and_then(|v| v.as_array())
    {
        for d in arr {
            if let Some(m) = d.get("message").and_then(|m| m.as_str()) {
                out.push(m.to_string());
            }
        }
    }
    out
}

/// The whole error object rendered to a string (for assertion messages).
pub fn error_text(env: &json::Value) -> String {
    env.get("error").map(|e| e.to_string()).unwrap_or_default()
}

// ---- source-tree helpers (small inline trees; larger inputs use fixtures) ----

pub fn write_source(dir: &std::path::Path, rel: &str, contents: &str) {
    let target = dir.join(rel);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    fs::write(&target, contents).expect("write source");
}

pub fn write_tree(dir: &std::path::Path, files: &[(&str, &str)]) {
    for (rel, contents) in files {
        write_source(dir, rel, contents);
    }
}

pub fn valid_function_source(args_schema: &str, result_schema: &str, body: &str) -> String {
    format!(
        "export def main [args: record<{args_schema}>]: nothing -> record<{result_schema}> {{\n{body}\n}}\n",
    )
}
