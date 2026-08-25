use serde_json::json;
use sourcetrait_grammar_mcp::guts::{TestServer, has_error};
use sourcetrait_common::testing::prelude::*;

/// One shared in-process server per test binary (substrate init must not race
/// itself). Every case below asserts CROSS-CALL state on the one serial interact
/// engine, so the whole file runs as one Stepper-sequenced test: interleaved
/// env/cd mutations from parallel siblings would break the persistence asserts.
static TESTING: testing::ModuleWith<TestServer> = testing::module_with!(Integration, {
    .setup(|_| TestServer::new())
});

#[tested]
fn interact_persistence_is_sequenced() {
    testing::Stepper::builder("interact_persistence")
        .init(|_t, _o: ()| testing::StepState((), ()))
        .step("env_mutation_persists_across_interact_calls", |t, s, p| {
            env_mutation_persists_across_interact_calls(t);
            testing::StepState(s, p)
        })
        .step("cd_persists_across_interact_calls", |t, s, p| {
            cd_persists_across_interact_calls(t);
            testing::StepState(s, p)
        })
        .step("interact_state_does_not_leak_into_run", |t, s, p| {
            interact_state_does_not_leak_into_run(t);
            testing::StepState(s, p)
        })
        .step("interact_cd_does_not_move_the_host_process_cwd", |t, s, p| {
            interact_cd_does_not_move_the_host_process_cwd(t);
            testing::StepState(s, p)
        })
        .step("multi_line_body_with_command_then_record_parses", |t, s, p| {
            multi_line_body_with_command_then_record_parses(t);
            testing::StepState(s, p)
        })
        .finalize()
        .run(TESTING.as_testable(), ());
}

fn env_mutation_persists_across_interact_calls(_t: &testing::Testable<'_, '_, '_>) {
    let s = TESTING.harness();
    let first = s.interact(
        json!({"value": "string"}),
        json!({"wrote": "string"}),
        json!({"value": "alpha"}),
        "$env.SHOT_DEMO = $args.value\n{ wrote: $args.value }",
    );
    assert_eq!(first["result"]["wrote"].as_str(), Some("alpha"));

    let second = s.interact(
        json!({"noop": "int"}),
        json!({"saw": "string"}),
        json!({"noop": 0}),
        "{ saw: $env.SHOT_DEMO }",
    );
    assert_eq!(
        second["result"]["saw"].as_str(),
        Some("alpha"),
        "the env mutation from call 1 should persist; got {second}",
    );
}

fn cd_persists_across_interact_calls(_t: &testing::Testable<'_, '_, '_>) {
    let s = TESTING.harness();
    let _ = s.interact(
        json!({"target": "string"}),
        json!({"cwd": "string"}),
        json!({"target": "/tmp"}),
        "cd $args.target\n{ cwd: (pwd) }",
    );
    let second = s.interact(
        json!({"noop": "int"}),
        json!({"cwd": "string"}),
        json!({"noop": 0}),
        "{ cwd: $env.PWD }",
    );
    assert_eq!(second["result"]["cwd"].as_str(), Some("/tmp"), "cd should propagate; got {second}");
}

fn interact_state_does_not_leak_into_run(_t: &testing::Testable<'_, '_, '_>) {
    let s = TESTING.harness();
    let _ = s.interact(
        json!({"noop": "int"}),
        json!({"ok": "bool"}),
        json!({"noop": 0}),
        "def leaked [] { 999 }\n{ ok: true }",
    );
    let env = s.run(
        json!({"noop": "int"}),
        json!({"out": "int"}),
        json!({"noop": 0}),
        "{ out: (leaked) }",
    );
    assert!(has_error(&env), "run() must NOT see interact()'s `leaked` def; got {env}");
}

/// nushell's `EngineState::merge_env` ends by chdir'ing the process to `$env.PWD`.
/// In-process that would move the whole grammar host for the rest of its life, so the
/// interact lane merges env WITHOUT that sync (server/embed.rs merge_env_no_chdir) and
/// `cd` stays engine state. Linux-only, like the rest of the /proc-based checks.
fn interact_cd_does_not_move_the_host_process_cwd(_t: &testing::Testable<'_, '_, '_>) {
    let s = TESTING.harness();
    let before = std::fs::read_link("/proc/self/cwd").expect("read /proc/self/cwd");
    let env = s.interact(
        json!({"dir": "string"}),
        json!({"pwd": "string"}),
        json!({"dir": "/etc"}),
        "cd $args.dir\n{ pwd: $env.PWD }",
    );
    assert_eq!(
        env["result"]["pwd"].as_str(),
        Some("/etc"),
        "cd should still set $env.PWD; got {env}",
    );
    let after = std::fs::read_link("/proc/self/cwd").expect("read /proc/self/cwd");
    assert_eq!(
        before, after,
        "an interact `cd` must not move the host process cwd - in-process there is no \
         worker subprocess to absorb it",
    );
}

fn multi_line_body_with_command_then_record_parses(_t: &testing::Testable<'_, '_, '_>) {
    let s = TESTING.harness();
    let env = s.interact(
        json!({"x": "int"}),
        json!({"y": "int", "slept_ms": "int"}),
        json!({"x": 7}),
        "sleep 50ms\nlet doubled = ($args.x * 2)\n{ y: $doubled, slept_ms: 50 }",
    );
    assert_eq!(env["result"]["y"].as_i64(), Some(14));
    assert_eq!(env["result"]["slept_ms"].as_i64(), Some(50));
}
