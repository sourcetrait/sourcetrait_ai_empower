use serde_json::json;
use sourcetrait_grammar_tests::*;
use sourcetrait_common::testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

#[tested]
fn deny_removes_tools_from_list() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn_args(t.temp_dir(), &["--deny", "run,interact,learn"]);
    let names = host.tool_names();
    assert_eq!(names.len(), 20, "23 - 3 denied = 20; got {names:?}");
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
        "rig",
        "channel_open",
        "channel_verified",
        "channel_close",
        "config_channel",
        "purviews",
        "purview_configure",
        "purview_extend",
        "purview",
        "info",
        "inspect",
        "processes",
        "kill",
        "remote_channel_open",
        "remote_channel_close",
        "remote_channels",
    ] {
        assert!(
            names.contains(&present.to_string()),
            "`{present}` should remain; got {names:?}",
        );
    }
}

#[tested]
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

#[tested]
fn deny_full_set_leaves_core_four() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn_args(
        t.temp_dir(),
        &[
            "--deny",
            "run,rerun,interact,call,learn,new,commit,rig,channel_open,channel_verified,channel_close,config_channel,purviews,purview_configure,purview_extend,purview,remote_channel_open,remote_channel_close,remote_channels",
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

#[tested]
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
