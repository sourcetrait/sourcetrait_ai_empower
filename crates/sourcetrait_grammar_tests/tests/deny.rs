use serde_json::json;
use sourcetrait_grammar_tests::*;
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

#[test]
#[named]
fn deny_removes_tools_from_list() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn_args(t.temp_dir(), &["--deny", "run,interact,learn"]);
    let names = host.tool_names();
    assert_eq!(names.len(), 12, "15 - 3 denied = 12; got {names:?}");
    for absent in ["run", "interact", "learn"] {
        assert!(
            !names.contains(&absent.to_string()),
            "denied `{absent}` must be absent; got {names:?}",
        );
    }
    for present in [
        "rerun",
        "call",
        "new",
        "commit",
        "library",
        "channel_open",
        "channel_verified",
        "channel_close",
        "info",
        "inspect",
        "processes",
        "kill",
    ] {
        assert!(
            names.contains(&present.to_string()),
            "`{present}` should remain; got {names:?}",
        );
    }
}

#[test]
#[named]
fn denied_tool_call_fails_at_protocol_layer() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn_args(t.temp_dir(), &["--deny", "run"]);
    let resp = host.call(
        "run",
        json!({
            "args_schema": {},
            "result_schema": {"out": "int"},
            "args": {},
            "body": "{ out: 1 }",
        }),
    );
    let success = resp
        .get("result")
        .and_then(|r| r.get("structuredContent"))
        .map(|sc| sc.get("error").is_none())
        .unwrap_or(false);
    assert!(!success, "a denied tool must not execute; got {resp}");
}

#[test]
#[named]
fn deny_full_set_leaves_core_four() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn_args(
        t.temp_dir(),
        &[
            "--deny",
            "run,rerun,interact,call,learn,new,commit,library,channel_open,channel_verified,channel_close",
        ],
    );
    let mut names = host.tool_names();
    names.sort();
    assert_eq!(
        names,
        vec!["info", "inspect", "kill", "processes"],
        "the full deny set leaves exactly the core four",
    );
}

#[test]
#[named]
fn unknown_deny_token_fails_startup() {
    let t = testing::test!({ .using_temp_dir() });
    let out = run_output(t.temp_dir(), &["--deny", "bogus"]);
    assert!(
        !out.status.success(),
        "unknown deny token should fail startup; got {:?}",
        out.status,
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("bogus"),
        "the clap error should name the bad token; got {stderr:?}",
    );
}
