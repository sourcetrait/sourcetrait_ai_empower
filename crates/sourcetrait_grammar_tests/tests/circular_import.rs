//! A circular module import must be a DIAGNOSTIC, never a dead host.
//!
//! SYSTEM tier by necessity rather than by preference. The condition under test kills
//! the process, so an in-process harness would take the test binary down with it and
//! leave nothing to assert on - only a spawned host can be watched from outside while
//! it dies. Both paths that can reach a cycle are covered: a body's direct `use`, and
//! the rig validator.

use std::time::Duration;

use serde_json::json;
use sourcetrait_grammar_tests::*;
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

/// The parent declares the child as a submodule.
const PARENT: &str = "export module inner\n\nexport def ping []: nothing -> string { \"parent\" }\n";

/// The child reaches back into the cascade it lives under - the cycle.
const CHILD: &str = "use ../mod.nu\n\nexport def pong []: nothing -> string { mod ping }\n";

/// Read the response, or fail naming HOW the host died rather than only that a read
/// failed. The death mode is the whole diagnostic value of this test while it is red.
fn expect_alive(
    host: &mut Host,
    id: u64,
    what: &str,
) -> serde_json::Value {
    match host.try_read_id(id) {
        Some(resp) => resp,
        None => {
            let how = host
                .wait_for_exit(Duration::from_secs(10))
                .map(|s| describe_exit(&s))
                .unwrap_or_else(|| "stdout closed but the process is still running".to_string());
            panic!(
                "the host died on {what} ({how}); a parse-level cycle must surface as a \
                 diagnostic, not take the process down",
            );
        }
    }
}

#[tested]
fn a_body_importing_a_cycle_gets_a_diagnostic_and_the_host_lives() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn(t.temp_dir());
    let root = t.temp_dir().join("circtest");
    write_source(&root, "mod.nu", PARENT);
    write_source(&root, "inner/mod.nu", CHILD);

    let body = format!("use {}\n{{ out: (circtest inner pong) }}", root.display());
    let id = host.request(
        "run",
        json!({
            "args_schema": {},
            "result_schema": {"out": "string"},
            "args": {},
            "body": body,
        }),
    );
    let resp = expect_alive(&mut host, id, "a body's circular `use`");

    assert!(
        has_error_path(&resp),
        "a circular import must come back as an error envelope; got {resp}",
    );
    // Its own KIND, not prose an agent would have to read. The raw parse errors for this
    // are unstable in variant and useless in every form, so the recogniser earning its
    // place IS the assertion.
    assert_eq!(
        envelope_error_kind(&resp),
        Some("module::circular_import"),
        "the cycle must arrive as its own kind; got {resp}",
    );

    // The point of the test: the host is still serving afterwards.
    let alive = host.call("info", json!({}));
    assert!(
        !has_error_path(&alive),
        "the host must still answer after a circular import; got {alive}",
    );
}

/// The DEPTH CEILING, not merely this one cycle.
///
/// Resolution appends a `/../<name>` segment per hop and gives up when the accumulated
/// path stops resolving, so depth is bounded by the filesystem's path limit divided by
/// the segment length - which means the SHORTEST legal module name is the worst case,
/// roughly doubling the hops the `inner` cycle reaches. The fix is a stack size, so the
/// claim it rests on is that the ceiling is finite and cleared. This measures that
/// rather than reasoning about it.
#[tested]
fn the_deepest_reachable_cycle_still_leaves_the_host_alive() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn(t.temp_dir());
    let root = t.temp_dir().join("c");
    write_source(&root, "mod.nu", "export module i\n");
    write_source(
        &root,
        "i/mod.nu",
        "use ../mod.nu\n\nexport def pong []: nothing -> string { \"deep\" }\n",
    );

    let body = format!("use {}\n{{ out: (c i pong) }}", root.display());
    let id = host.request(
        "run",
        json!({
            "args_schema": {},
            "result_schema": {"out": "string"},
            "args": {},
            "body": body,
        }),
    );
    let resp = expect_alive(&mut host, id, "the deepest reachable cycle");

    assert!(
        has_error_path(&resp),
        "the deep cycle must come back as an error envelope; got {resp}",
    );
    let alive = host.call("info", json!({}));
    assert!(
        !has_error_path(&alive),
        "the host must still answer after the deepest cycle; got {alive}",
    );
}

/// The one-shot CLI reaches the same parse by a different route, and it is the route
/// that is easy to leave unprotected: the serve path runs handlers on runtime workers,
/// while the CLI's future is driven on whichever thread called `block_on`. Locked
/// separately because a fix covering only the serve path passes every other test here.
#[tested]
fn the_one_shot_cli_survives_a_cycle_too() {
    let t = testing::test!({ .using_temp_dir() });
    let root = t.temp_dir().join("circtest");
    write_source(&root, "mod.nu", PARENT);
    write_source(&root, "inner/mod.nu", CHILD);

    let body = format!("use {}\n{{ out: (circtest inner pong) }}", root.display());
    let out = run_output(
        t.temp_dir(),
        &[
            "--id",
            "clicirc",
            "cli",
            "run",
            &body,
            "--args-schema",
            "{}",
            "--result-schema",
            "{out: string}",
            "--args",
            "{}",
        ],
    );

    // `code()` is None when a process is killed by a signal, so this is what tells a
    // reported error apart from a core dump.
    assert_eq!(
        out.status.code(),
        Some(1),
        "the cli must report an error envelope rather than die; got {:?}",
        out.status,
    );
    let v = stdout_json(&out);
    assert!(v.get("error").is_some(), "expected an error envelope; got {v}");
    assert_eq!(
        v["error"]["errors"][0]["kind"].as_str(),
        Some("module::circular_import"),
        "the cli must report the same kind; got {v}",
    );
}

#[tested]
fn a_rig_carrying_a_cycle_fails_validation_and_the_host_lives() {
    let t = testing::test!({ .using_temp_dir() });
    let mut host = Host::spawn(t.temp_dir());
    let src = t.temp_dir().join("circlib");
    let est = host.rig_new("sourcetrait/circlib", &src);
    assert!(!has_error_path(&est), "rig new should succeed; got {est}");

    // The same cycle, one level down, with the child also being a call target - the
    // shape a real rig would carry.
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export module inner\nexport use inner\n");
    write_source(
        &src,
        "m/inner/mod.nu",
        "use ../mod.nu\n\nexport def main [args: record<x: int>]: nothing -> record<out: int> {\n    { out: $args.x }\n}\n",
    );

    let id = host.request(
        "rig",
        json!({
            "action": "check",
            "rig": "sourcetrait/circlib",
            "source_dir": src.to_str().unwrap(),
        }),
    );
    let resp = expect_alive(&mut host, id, "a rig validated with a circular `use`");

    // `rig check` reports through its SUCCESS summary rather than an error envelope, so
    // the diagnostic lives in summary.errors rather than where the other cases carry it.
    let kinds: Vec<String> = structured(&resp)["summary"]["errors"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|e| e["kind"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        kinds.iter().any(|k| k == "module::circular_import"),
        "the validator must name the cycle; got {kinds:?} in {resp}",
    );

    // And the host must outlive deciding it.
    let alive = host.call("info", json!({}));
    assert!(
        !has_error_path(&alive),
        "the host must still answer after validating a cyclic rig; got {alive}",
    );
}
