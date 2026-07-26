//! System-test harness for `grammar_mcp`.
//!
//! These tests spawn the REAL host binary - the one-shot `cli` subcommand or the
//! JSON-RPC server over stdio - so they are SYSTEM tests and live outside the mcp
//! crate to keep its own `cargo test` fast. The binary is located from the test
//! binary's own path (`current_exe()` -> the `target/<profile>/grammar_mcp`
//! sibling of `deps/`), so it must be built first:
//! `cargo build -p sourcetrait_grammar_mcp` (or `cargo test --workspace`).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// The `grammar_mcp` host binary, resolved as the workspace-bin sibling of the
/// running test binary's `deps/` dir.
pub fn grammar_mcp_bin() -> PathBuf {
    let mut dir = std::env::current_exe().expect("current_exe");
    dir.pop(); // drop the test binary file name -> .../deps
    if dir.ends_with("deps") {
        dir.pop(); // -> .../<profile>
    }
    dir.join("grammar_mcp")
}

/// Run the binary as a one-shot subprocess (the `cli` surface, or a
/// startup-failure check), isolating XDG state under `scratch/{data,cache}`.
/// Returns the captured Output (the `cli` surface prints bare compact JSON).
pub fn run_output(scratch: &Path, args: &[&str]) -> Output {
    let data = scratch.join("data");
    let cache = scratch.join("cache");
    std::fs::create_dir_all(&data).expect("mkdir data");
    std::fs::create_dir_all(&cache).expect("mkdir cache");
    Command::new(grammar_mcp_bin())
        .args(args)
        .env("XDG_DATA_HOME", &data)
        .env("XDG_CACHE_HOME", &cache)
        .output()
        .expect("run grammar_mcp")
}

pub fn stdout_str(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn stdout_json(out: &Output) -> Value {
    let text = stdout_str(out);
    serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("stdout should be JSON: {e}; got {text:?}"))
}

/// A spawned `grammar_mcp` server child, driven over JSON-RPC stdio. Killed on
/// Drop.
pub struct Host {
    child: Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
    data_home: PathBuf,
    cache_home: PathBuf,
    /// Responses read while waiting for a different id, kept by id so a caller can
    /// fire N concurrent requests and read all N back (read_id would otherwise
    /// discard any response whose id it is not currently waiting for).
    pending: HashMap<u64, Value>,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Host {
    /// Spawn with XDG state under `scratch/{data,cache}` and no extra CLI args.
    pub fn spawn(scratch: &Path) -> Self {
        Self::spawn_full(scratch, &[], &[])
    }

    /// Spawn with extra CLI args (e.g. `--deny`, `--id`, `--namespace`).
    pub fn spawn_args(scratch: &Path, args: &[&str]) -> Self {
        Self::spawn_full(scratch, args, &[])
    }

    /// Spawn with extra CLI args AND extra env vars (e.g. `USER`, `HOME`).
    pub fn spawn_full(scratch: &Path, args: &[&str], envs: &[(&str, &str)]) -> Self {
        let data_home = scratch.join("data");
        let cache_home = scratch.join("cache");
        std::fs::create_dir_all(&data_home).expect("mkdir data");
        std::fs::create_dir_all(&cache_home).expect("mkdir cache");
        let mut cmd = Command::new(grammar_mcp_bin());
        cmd.args(args)
            .env("XDG_DATA_HOME", &data_home)
            .env("XDG_CACHE_HOME", &cache_home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        for (k, v) in envs {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().expect("spawn grammar_mcp");
        let stdin = child.stdin.take().expect("host stdin");
        let stdout = BufReader::new(child.stdout.take().expect("host stdout"));
        let mut host = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
            data_home,
            cache_home,
            pending: HashMap::new(),
        };
        host.initialize();
        host
    }

    /// The spawned host's pid - for tests that inspect its process tree from /proc
    /// (e.g. that it reaps the orphans the subreaper adopts).
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// The XDG_DATA_HOME the child was spawned with (root of its namespace subtree).
    pub fn data_home(&self) -> &Path {
        &self.data_home
    }

    /// The XDG_CACHE_HOME the child was spawned with.
    pub fn cache_home(&self) -> &Path {
        &self.cache_home
    }

    /// Signal the host and wait for it to exit, returning its status.
    ///
    /// The distinction this exists to measure: a host whose handler RAN exits on its
    /// own terms (`code() == Some(128 + signo)`), while one that merely took the
    /// signal's default action is reported as killed BY the signal (`code() == None`,
    /// `signal() == Some(signo)`). Only the former proves the shutdown sweep ran.
    pub fn signal_and_wait(
        &mut self,
        signal: nix::sys::signal::Signal,
        timeout: Duration,
    ) -> Option<std::process::ExitStatus> {
        let pid = nix::unistd::Pid::from_raw(self.child.id() as i32);
        nix::sys::signal::kill(pid, Some(signal)).expect("signal the host");
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait().expect("try_wait") {
                Some(status) => return Some(status),
                None if Instant::now() >= deadline => return None,
                None => std::thread::sleep(Duration::from_millis(25)),
            }
        }
    }

    /// `read_id`, but tolerant of the host DYING mid-request.
    ///
    /// `read_id` panics on EOF, which is the right default for a test whose host is
    /// expected to answer. A test asserting that some input does NOT kill the host
    /// needs the other behaviour: EOF is the OBSERVATION, and panicking on it throws
    /// away the chance to report what actually happened. Pair it with
    /// `wait_for_exit` + `describe_exit` to name the death mode.
    pub fn try_read_id(
        &mut self,
        expected_id: u64,
    ) -> Option<Value> {
        if let Some(msg) = self.pending.remove(&expected_id) {
            return Some(msg);
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if Instant::now() >= deadline {
                return None;
            }
            let mut line = String::new();
            match self.stdout.read_line(&mut line) {
                // EOF: the host's stdout closed, which from here is what a dead host
                // looks like.
                Ok(0) | Err(_) => return None,
                Ok(_) => {}
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };
            match msg.get("id").and_then(|v| v.as_u64()) {
                Some(id) if id == expected_id => return Some(msg),
                Some(id) => {
                    self.pending.insert(id, msg);
                }
                None => {}
            }
        }
    }

    /// Wait for the child to exit on its own, returning its status; None on timeout.
    ///
    /// Unlike `signal_and_wait` this sends nothing - it observes a host that is
    /// already on its way out.
    pub fn wait_for_exit(
        &mut self,
        timeout: Duration,
    ) -> Option<std::process::ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait().expect("try_wait") {
                Some(status) => return Some(status),
                None if Instant::now() >= deadline => return None,
                None => std::thread::sleep(Duration::from_millis(25)),
            }
        }
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn send(&mut self, msg: &Value) {
        let line = msg.to_string();
        self.stdin.write_all(line.as_bytes()).expect("write line");
        self.stdin.write_all(b"\n").expect("write newline");
        self.stdin.flush().expect("flush");
    }

    pub fn read_id(&mut self, expected_id: u64) -> Value {
        if let Some(msg) = self.pending.remove(&expected_id) {
            return msg;
        }
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
            let msg: Value = serde_json::from_str(trimmed)
                .unwrap_or_else(|e| panic!("parse JSON: {e} from {trimmed:?}"));
            match msg.get("id").and_then(|v| v.as_u64()) {
                Some(id) if id == expected_id => return msg,
                // A response for another in-flight request (pipelined / concurrent
                // reads): buffer it by id rather than drop it, so a later
                // read_id(that_id) finds it.
                Some(id) => {
                    self.pending.insert(id, msg);
                }
                // A notification carries no id; none arrive post-init - skip.
                None => {}
            }
        }
    }

    fn initialize(&mut self) {
        let id = self.next_id();
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "grammar-system-tests", "version": "0.0.1"}
            }
        }));
        let _ = self.read_id(id);
        self.send(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }));
    }

    /// Send a `tools/call` WITHOUT reading its response, returning the request id -
    /// for pipelined / concurrent requests (e.g. kill mid-run). Read the response
    /// later with `read_id(id)`. Author-prefixes bare rig names (idempotent).
    pub fn request(&mut self, tool: &str, args: Value) -> u64 {
        let id = self.next_id();
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": tool, "arguments": author_prefixed(tool, args)}
        }));
        id
    }

    /// A `tools/call`, author-prefixing bare rig names (idempotent for
    /// values already carrying an `author/name`).
    pub fn call(&mut self, tool: &str, args: Value) -> Value {
        let id = self.request(tool, args);
        self.read_id(id)
    }

    pub fn run(&mut self, args: Value) -> Value {
        self.call("run", args)
    }

    pub fn call_np(&mut self, namepath: &str, args: Value) -> Value {
        self.call("call", json!({"namepath": namepath, "args": args}))
    }

    pub fn rig(&mut self, action: &str, name: &str, source_dir: &str) -> Value {
        self.call(
            "rig",
            json!({"action": action, "rig": name, "source_dir": source_dir}),
        )
    }

    pub fn rig_new(&mut self, name: &str, src: &Path) -> Value {
        self.rig("new", name, src.to_str().unwrap())
    }

    pub fn commit(&mut self, name: &str) -> Value {
        self.call("commit", json!({"rig": name}))
    }

    /// The tool names advertised by `tools/list`.
    pub fn tool_names(&mut self) -> Vec<String> {
        let id = self.next_id();
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": "tools/list"}));
        let resp = self.read_id(id);
        resp["result"]["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .map(|t| t["name"].as_str().expect("tool name").to_string())
            .collect()
    }
}

/// Prepend `sourcetrait/` to a bare rig name in the args of the
/// rig-addressing tools. Idempotent: a value already carrying `/` (an
/// `author/name`) passes through unchanged.
pub fn author_prefixed(tool: &str, mut args: Value) -> Value {
    fn pfx_lib(s: &str) -> String {
        if s.is_empty() || s.contains('/') {
            s.to_string()
        } else {
            format!("sourcetrait/{s}")
        }
    }
    fn pfx_np(s: &str) -> String {
        let lib = s.split(':').next().unwrap_or(s);
        if lib.is_empty() || lib.contains('/') {
            s.to_string()
        } else {
            format!("sourcetrait/{s}")
        }
    }
    match tool {
        "call" | "inspect" => {
            if let Some(np) = args.get("namepath").and_then(|v| v.as_str()) {
                args["namepath"] = Value::String(pfx_np(np));
            }
        }
        "new" => {
            if let Some(arr) = args.get_mut("namepaths").and_then(|v| v.as_array_mut()) {
                for e in arr.iter_mut() {
                    if let Some(s) = e.as_str() {
                        *e = Value::String(pfx_np(s));
                    }
                }
            }
        }
        "rig" | "commit" => {
            if let Some(l) = args.get("rig").and_then(|v| v.as_str()) {
                args["rig"] = Value::String(pfx_lib(l));
            }
        }
        _ => {}
    }
    args
}

/// How a child ended, in a form a failure message can carry.
///
/// The distinction is the whole point: a host that EXITED chose to, and its code says
/// why; a host reported as killed BY a signal was taken down by the kernel, and the
/// signal names the mechanism (SIGSEGV = a memory fault, which for a parser means a
/// stack overflow far more often than anything else).
pub fn describe_exit(status: &std::process::ExitStatus) -> String {
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("exited with code {code}"),
        (None, Some(sig)) => {
            let name = match sig {
                6 => " (SIGABRT)",
                9 => " (SIGKILL)",
                11 => " (SIGSEGV)",
                15 => " (SIGTERM)",
                _ => "",
            };
            format!("killed by signal {sig}{name}")
        }
        (None, None) => "ended for an unknown reason".to_string(),
    }
}

// ---- envelope / error helpers ----

pub fn structured(resp: &Value) -> &Value {
    &resp["result"]["structuredContent"]
}

pub fn envelope_error(resp: &Value) -> Option<&Value> {
    resp.get("result")?.get("structuredContent")?.get("error")
}

pub fn has_error_path(resp: &Value) -> bool {
    envelope_error(resp).is_some()
}

pub fn envelope_error_kind(resp: &Value) -> Option<&str> {
    envelope_error(resp)?
        .get("errors")?
        .as_array()?
        .first()?
        .get("kind")?
        .as_str()
}

pub fn error_kinds(resp: &Value) -> Vec<String> {
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

pub fn has_kind(resp: &Value, kind: &str) -> bool {
    error_kinds(resp).iter().any(|k| k == kind)
}

pub fn error_message(resp: &Value) -> String {
    envelope_error(resp)
        .map(|e| e.to_string())
        .unwrap_or_else(|| resp.to_string())
}

/// The success envelope, or `None` when the response carries an `error`.
pub fn extract_envelope(resp: &Value) -> Option<Value> {
    let sc = resp.get("result")?.get("structuredContent")?.clone();
    if sc.get("error").is_some() {
        return None;
    }
    Some(sc)
}

// ---- source-tree fixtures (written into the per-test scratch) ----

pub fn write_source(dir: &Path, rel: &str, contents: &str) {
    let target = dir.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(&target, contents).expect("write source");
}

pub fn valid_function_source(args_schema: &str, result_schema: &str, body: &str) -> String {
    format!(
        "export def main [args: record<{args_schema}>]: nothing -> record<{result_schema}> {{\n{body}\n}}\n",
    )
}

/// The on-disk namespace subtree for `(id, namespace)` under a data home.
pub fn namespace_dir(data_home: &Path, id: &str, namespace: &str) -> PathBuf {
    data_home
        .join("sourcetrait")
        .join("grammar")
        .join(id)
        .join(namespace)
}
