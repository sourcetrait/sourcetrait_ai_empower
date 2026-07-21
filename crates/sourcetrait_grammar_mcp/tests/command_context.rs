use serde_json::json;
use sourcetrait_grammar_mcp::guts::{TestServer, lint_engine_reuses_until_the_registry_moves};

/// The validator engine is rebuilt on a plugin-registry change, not on every
/// call. Integration rather than unit: constructing one builds a whole shell
/// command context and reads the registry off disk.
#[test]
fn lint_engine_does_not_rebuild_when_the_registry_is_unchanged() {
    assert!(
        lint_engine_reuses_until_the_registry_moves(),
        "current() must hand back the SAME engine while the plugin registry is \
         unchanged; rebuilding per call would pay a full command-context build \
         on every lint and every commit",
    );
}

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
