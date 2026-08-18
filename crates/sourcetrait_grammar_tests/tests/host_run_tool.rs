use serde_json::json;
use sourcetrait_grammar_tests::*;
use sourcetrait_common::testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

/// initialize -> tools/list (exactly 23) -> a run() round-trip. The rmcp stdio
/// transport + handshake are the system surface here.
#[tested]
fn tools_list_and_run_round_trip() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn(t.temp_dir());

    let names = host.tool_names();
    assert_eq!(names.len(), 23, "expected 23 tools; got {names:?}");
    for expected in [
        "run",
        "interact",
        "rerun",
        "call",
        "processes",
        "kill",
        "info",
        "learn",
        "new",
        "commit",
        "rig",
        "inspect",
        "channel_open",
        "channel_verified",
        "channel_close",
        "config_channel",
        "purviews",
        "purview_configure",
        "purview_extend",
        "purview",
        "remote_channel_open",
        "remote_channel_close",
        "remote_channels",
    ] {
        assert!(
            names.contains(&expected.to_string()),
            "missing `{expected}` in {names:?}",
        );
    }

    let resp = host.run(json!({
        "args_schema": {"x": "int"},
        "result_schema": {"out": "int"},
        "args": {"x": 5},
        "body": "{ out: ($args.x + 1) }",
    }));
    let env = structured(&resp);
    let nonce = env["nonce"].as_str().expect("envelope nonce");
    assert!(
        !nonce.is_empty() && nonce.chars().all(|c| c.is_ascii_alphanumeric()),
        "nonce should be base62; got {nonce:?}",
    );
    assert_eq!(env["result"]["out"].as_i64(), Some(6), "got {env}");
}
