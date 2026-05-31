use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn send(stdin: &mut std::process::ChildStdin, msg: &serde_json::Value) {
    let line = msg.to_string();
    stdin.write_all(line.as_bytes()).expect("write line");
    stdin.write_all(b"\n").expect("write newline");
    stdin.flush().expect("flush");
}

fn read_response(
    stdout: &mut BufReader<std::process::ChildStdout>,
    expected_id: u64,
) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if Instant::now() >= deadline {
            panic!("timed out waiting for JSON-RPC response id {expected_id}");
        }
        let mut line = String::new();
        let n = stdout.read_line(&mut line).expect("read line");
        if n == 0 {
            panic!("EOF on host stdout while waiting for id {expected_id}");
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let msg: serde_json::Value = serde_json::from_str(trimmed)
            .unwrap_or_else(|e| panic!("parse JSON-RPC line {trimmed:?}: {e}"));
        if msg.get("id").and_then(|v| v.as_u64()) == Some(expected_id) {
            return msg;
        }
    }
}

#[test]
fn host_tools_list_and_run_stub() {
    let host_bin = env!("CARGO_BIN_EXE_nu_sh_mcp");
    let worker_bin = env!("CARGO_BIN_EXE_nu_sh_mcp_worker");

    let data_dir = tempfile::tempdir().expect("data tempdir");
    let cache_dir = tempfile::tempdir().expect("cache tempdir");
    let host_spawn_start = Instant::now();
    let mut host = Command::new(host_bin)
        .env("NU_SH_MCP_WORKER_PATH", worker_bin)
        .env("XDG_DATA_HOME", data_dir.path())
        .env("XDG_CACHE_HOME", cache_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn host");
    let mut stdin = host.stdin.take().expect("host stdin pipe");
    let mut stdout = BufReader::new(host.stdout.take().expect("host stdout pipe"));

    // initialize
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "nu_sh_mcp_integration_test", "version": "0.0.1"}
        }
    });
    send(&mut stdin, &init);
    let init_resp = read_response(&mut stdout, 1);
    let init_elapsed = host_spawn_start.elapsed();
    assert!(
        init_resp.get("result").is_some(),
        "initialize response should have result; got {init_resp}",
    );
    eprintln!(
        "host spawn -> initialize response: {:.3} ms",
        init_elapsed.as_secs_f64() * 1000.0,
    );

    // initialized notification (no id)
    let initialized = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
    });
    send(&mut stdin, &initialized);

    // tools/list
    let list = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
    });
    send(&mut stdin, &list);
    let list_resp = read_response(&mut stdout, 2);
    let tools = list_resp["result"]["tools"]
        .as_array()
        .expect("tools array in tools/list result");
    assert_eq!(tools.len(), 9, "expected exactly 9 tools, got {tools:?}");
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().expect("tool name"))
        .collect();
    for expected in [
        "run",
        "interact",
        "rerun",
        "register_library",
        "unregister_library",
        "define_function",
        "undefine_function",
        "import_library",
        "reimport_library",
    ] {
        assert!(
            names.contains(&expected),
            "missing `{expected}` in {names:?}",
        );
    }

    // tools/call run with a stub closure body
    let call_start = Instant::now();
    let call = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "run",
            "arguments": {
                "args_schema": "x: int",
                "result_schema": "out: int",
                "args": {"x": 5},
                "closure": "{ out: ($args.x + 1) }",
                "functions": []
            }
        }
    });
    send(&mut stdin, &call);
    let call_resp = read_response(&mut stdout, 3);
    let call_elapsed = call_start.elapsed();
    let result = call_resp
        .get("result")
        .unwrap_or_else(|| panic!("tools/call response missing result: {call_resp}"));
    let content = result["content"]
        .as_array()
        .expect("content array in tools/call result");
    assert!(!content.is_empty(), "content array should not be empty");
    let text = content[0]["text"]
        .as_str()
        .expect("first content block has text");
    let envelope: serde_json::Value = serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("envelope parses as JSON ({e}): {text:?}"));
    // 0.0.9+: rerun_id is a content-derived base62 hash, not the
    // pre-cache placeholder "0". Spot-check shape: non-empty,
    // alphanumeric.
    let rerun_id = envelope["rerun_id"]
        .as_str()
        .expect("envelope has rerun_id");
    assert!(
        !rerun_id.is_empty() && rerun_id.chars().all(|c| c.is_ascii_alphanumeric()),
        "rerun_id should be non-empty base62; got {rerun_id:?}",
    );
    // 0.0.7+: result is a structured JSON object (via nu_json::Value
    // conversion in the worker), not a NUON string. Closure
    // `{ out: ($args.x + 1) }` with args.x = 5 returns the record
    // {out: 6}, surfaced as JSON object {"out": 6}.
    assert_eq!(
        envelope["result"]["out"].as_i64(),
        Some(6),
        "result.out should be 6; got {:?}",
        envelope["result"],
    );
    eprintln!(
        "tools/call run -> stub envelope: {:.3} ms",
        call_elapsed.as_secs_f64() * 1000.0,
    );

    // clean shutdown
    drop(stdin);
    let _ = wait_with_timeout(&mut host, Duration::from_secs(5));
}

fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> std::io::Result<()> {
    let start = Instant::now();
    loop {
        match child.try_wait()? {
            Some(_) => return Ok(()),
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}
