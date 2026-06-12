use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn read_frame<R: Read>(r: &mut R) -> Vec<u8> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).expect("read length");
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload).expect("read payload");
    payload
}

fn write_frame<W: Write>(w: &mut W, payload: &[u8]) {
    let len = payload.len() as u32;
    w.write_all(&len.to_le_bytes()).expect("write length");
    w.write_all(payload).expect("write payload");
    w.flush().expect("flush");
}

#[test]
fn worker_handshake_and_stub_response() {
    let worker_bin = env!("CARGO_BIN_EXE_nushell_mcp_worker");
    let spawn_start = Instant::now();
    let mut child = Command::new(worker_bin)
        .arg("--mode")
        .arg("stateless")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn nushell_mcp_worker");
    let mut stdin = child.stdin.take().expect("stdin pipe");
    let mut stdout = child.stdout.take().expect("stdout pipe");

    let hello_frame = read_frame(&mut stdout);
    let hello_elapsed = spawn_start.elapsed();
    let hello: serde_json::Value =
        rmp_serde::from_slice(&hello_frame).expect("decode Hello");
    assert_eq!(hello["protocol_version"].as_u64(), Some(1));
    eprintln!(
        "cold worker startup -> Hello: {:.3} ms",
        hello_elapsed.as_secs_f64() * 1000.0,
    );

    let log_dir = std::env::temp_dir().join("nushell_mcp_handshake_test");
    std::fs::create_dir_all(&log_dir).expect("create test log_dir");
    let request = serde_json::json!({
        "id": 42u64,
        "log_dir": log_dir.to_str().expect("log_dir to utf-8"),
        "source": "1 + 1",
    });
    let request_bytes = rmp_serde::to_vec_named(&request)
        .expect("encode RunRequest");
    write_frame(&mut stdin, &request_bytes);

    let rt_start = Instant::now();
    let response_frame = read_frame(&mut stdout);
    let rt_elapsed = rt_start.elapsed();
    let response: serde_json::Value =
        rmp_serde::from_slice(&response_frame).expect("decode RunResponse");
    assert_eq!(response["id"].as_u64(), Some(42));
    assert_eq!(response["ok"].as_bool(), Some(true), "worker error: {:?}", response["error"]);
    assert!(response["error"].is_null());
    eprintln!(
        "IPC round-trip (RunRequest -> RunResponse, real eval `1 + 1`): {:.3} ms",
        rt_elapsed.as_secs_f64() * 1000.0,
    );

    // 0.0.7+: value field is a msgpack-encoded JSON value (was a NUON
    // string before). `1 + 1` evaluates to a nushell int, which converts
    // to a JSON Number(2), which msgpacks as an integer.
    let value_bytes: Vec<u8> = response["value"]
        .as_array()
        .expect("value is byte array")
        .iter()
        .map(|v| v.as_u64().expect("byte") as u8)
        .collect();
    let value_json: serde_json::Value = rmp_serde::from_slice(&value_bytes)
        .expect("decode value as msgpack JSON value");
    assert_eq!(
        value_json.as_i64(),
        Some(2),
        "real eval of `1 + 1` should yield JSON 2; got {value_json:?}",
    );

    drop(stdin);
    let _ = child.wait_timeout_or_kill(Duration::from_secs(5));
}

trait ChildExt {
    fn wait_timeout_or_kill(&mut self, timeout: Duration) -> std::io::Result<()>;
}

impl ChildExt for std::process::Child {
    fn wait_timeout_or_kill(&mut self, timeout: Duration) -> std::io::Result<()> {
        let start = Instant::now();
        loop {
            match self.try_wait()? {
                Some(_) => return Ok(()),
                None => {
                    if start.elapsed() >= timeout {
                        let _ = self.kill();
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
    }
}
