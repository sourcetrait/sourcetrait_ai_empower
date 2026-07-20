use serde_json::json;
use sourcetrait_grammar_mcp::guts::TestServer;

/// run() carries nu-cmd-extra (bits / str-case) but NOT the `plugin *` admin
/// family; interact() carries both.
#[test]
fn run_has_extra_lacks_plugin() {
    let s = TestServer::new();
    let env = s.run(
        json!({}),
        json!({"bits": "bool", "snake": "bool", "plugin": "bool", "bits_and": "int"}),
        json!({}),
        "let names = (scope commands | get name)\n{ bits: (\"bits and\" in $names), snake: (\"str snake-case\" in $names), plugin: (\"plugin list\" in $names), bits_and: (5 | bits and 3) }",
    );
    assert_eq!(env["result"]["bits"].as_bool(), Some(true), "run should have `bits and`; got {env}");
    assert_eq!(env["result"]["snake"].as_bool(), Some(true), "run should have `str snake-case`; got {env}");
    assert_eq!(env["result"]["plugin"].as_bool(), Some(false), "run must NOT have `plugin *`; got {env}");
    assert_eq!(env["result"]["bits_and"].as_i64(), Some(1), "5 & 3 = 1; got {env}");
}

#[test]
fn interact_has_extra_and_plugin() {
    let s = TestServer::new();
    let env = s.interact(
        json!({}),
        json!({"bits": "bool", "plugin": "bool"}),
        json!({}),
        "let names = (scope commands | get name)\n{ bits: (\"bits and\" in $names), plugin: (\"plugin list\" in $names) }",
    );
    assert_eq!(env["result"]["bits"].as_bool(), Some(true), "interact should have `bits and`; got {env}");
    assert_eq!(env["result"]["plugin"].as_bool(), Some(true), "interact should have `plugin *`; got {env}");
}
