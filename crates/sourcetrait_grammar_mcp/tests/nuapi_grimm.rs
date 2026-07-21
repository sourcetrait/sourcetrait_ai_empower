//! The embedded API (`grimm *`) - decls registered per-eval into the working set
//! `eval_in_process` parses with, so they exist ONLY inside a run/call/interact
//! body and carry that call's own log dir.

use serde_json::json;
use sourcetrait_grammar_mcp::guts::{TestServer, error_text, has_error};

/// Read `debug.nuonl` back THROUGH nushell, so the assertion covers the round-trip
/// (the file parses as NUON) and not merely the bytes we wrote.
fn read_back(s: &TestServer, nonce: &str, result_schema: serde_json::Value, body: &str) -> serde_json::Value {
    let path = s.run_log_dir(nonce).join("debug.nuonl");
    s.run(
        json!({"path": "string"}),
        result_schema,
        json!({"path": path.to_str().expect("utf-8 path")}),
        body,
    )
}

#[test]
fn dbg_and_channel_send_each_append_one_line() {
    let s = TestServer::new();
    let env = s.run(
        json!({}),
        json!({"ok": "bool"}),
        json!({}),
        "grimm dbg {a: 1, b: \"two\"}\ngrimm channel_send [[x, y]; [1, 2]]\n{ ok: true }",
    );
    assert!(!has_error(&env), "body should run; got {env}");
    let nonce = env["nonce"].as_str().expect("nonce");

    let back = read_back(
        &s,
        nonce,
        json!({"count": "int", "first_a": "int", "second_x": "int"}),
        "let recs = (open --raw $args.path | decode | lines | each {|l| $l | from nuon })\n\
         { count: ($recs | length), first_a: ($recs | get 0 | get a), second_x: ($recs | get 1 | get 0.x) }",
    );
    assert_eq!(back["result"]["count"].as_i64(), Some(2), "one line per call; got {back}");
    assert_eq!(back["result"]["first_a"].as_i64(), Some(1), "got {back}");
    assert_eq!(
        back["result"]["second_x"].as_i64(),
        Some(1),
        "channel_send writes the table verbatim (stubbed to dbg); got {back}",
    );
}

/// `to nuon` emits a newline INSIDE a string raw, which would split one record
/// across lines and break the nuonl invariant. The escape must survive a
/// round-trip, and must not disturb the fields after it.
#[test]
fn a_record_with_an_embedded_newline_stays_one_line() {
    let s = TestServer::new();
    let env = s.run(
        json!({}),
        json!({"ok": "bool"}),
        json!({}),
        "grimm dbg {msg: (\"line one\" + (char nl) + \"line two\"), tail: 9}\n{ ok: true }",
    );
    assert!(!has_error(&env), "got {env}");
    let nonce = env["nonce"].as_str().expect("nonce");

    let back = read_back(
        &s,
        nonce,
        json!({"lines": "int", "msg": "string", "tail": "int"}),
        "let ls = (open --raw $args.path | decode | lines)\n\
         let rec = ($ls | first | from nuon)\n\
         { lines: ($ls | length), msg: $rec.msg, tail: $rec.tail }",
    );
    assert_eq!(back["result"]["lines"].as_i64(), Some(1), "must not span lines; got {back}");
    assert_eq!(
        back["result"]["msg"].as_str(),
        Some("line one\nline two"),
        "the escaped newline must round-trip to the original string; got {back}",
    );
    assert_eq!(
        back["result"]["tail"].as_i64(),
        Some(9),
        "the escape must not disturb following fields; got {back}",
    );
}

/// A literal is rejected by the signature's `oneof<record, table>` shape at parse
/// time; a DYNAMIC value reaches `run` unchecked, which is what the runtime guard
/// is for. Both must fail.
#[test]
fn a_non_record_is_rejected() {
    let s = TestServer::new();
    for body in ["grimm dbg 5\n{ ok: true }", "let x = 5\ngrimm dbg $x\n{ ok: true }"] {
        let env = s.run(json!({}), json!({"ok": "bool"}), json!({}), body);
        assert!(has_error(&env), "`{body}` should be rejected; got {env}");
        let text = error_text(&env).to_lowercase();
        assert!(
            text.contains("record") || text.contains("table"),
            "the rejection should name the accepted shapes; got {}",
            error_text(&env),
        );
    }
}

#[test]
fn the_api_is_available_to_interact_too() {
    let s = TestServer::new();
    let env = s.interact(
        json!({}),
        json!({"ok": "bool"}),
        json!({}),
        "grimm dbg {lane: \"interact\"}\n{ ok: true }",
    );
    assert!(!has_error(&env), "interact should carry the embedded API; got {env}");
}
