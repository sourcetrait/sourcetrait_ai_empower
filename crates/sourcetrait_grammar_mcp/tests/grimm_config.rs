//! The `grimm` config surface, driven from real eval bodies.
//!
//! In-process is the right tier for these, and not merely the cheap one: the pin
//! registry and `CONFIG` are process-globals, and `TestServer` shares the very
//! process the eval runs in, so a test can pin to its own pid and know for
//! certain the process is alive - which is the one thing a spawned host could not
//! give it.

use serde_json::json;
use sourcetrait_grammar_mcp::guts::{
    TestServer, clear_config_pins, error_text, has_error,
};

/// The pid the eval actually runs under. Eval is in-process, so the test's own
/// pid IS the host's.
fn me() -> u32 {
    std::process::id()
}

#[test]
fn get_config_all_mirrors_the_toml_tables() {
    let s = TestServer::new();
    let env = s.run(
        json!({}),
        json!({"tables": ["string"], "channel_keys": ["string"], "supervisor_keys": ["string"]}),
        json!({}),
        "let cfg = (grimm get_config_all)\n\
         { tables: ($cfg | columns), channel_keys: ($cfg.channel | columns), \
           supervisor_keys: ($cfg.supervisor | columns) }",
    );
    assert!(!has_error(&env), "get_config_all should run; got {env}");
    assert_eq!(
        env["result"]["tables"],
        json!(["channel", "supervisor"]),
        "the record mirrors the TOML's two tables; got {env}",
    );
    assert_eq!(
        env["result"]["channel_keys"],
        json!([
            "port",
            "cert_dir",
            "spam_warn_window_secs",
            "spam_warn_rate",
            "spam_error_window_secs",
            "spam_error_rate",
        ]),
        "got {env}",
    );
    assert_eq!(
        env["result"]["supervisor_keys"],
        json!([
            "cpu_warn_fraction",
            "ram_warn_fraction",
            "vram_warn_headroom_mib",
            "disk_warn_fraction",
        ]),
        "got {env}",
    );
}

#[test]
fn a_dotted_key_is_a_path_into_the_record() {
    let s = TestServer::new();
    // The coherence property: the key is a path into what get_config_all returns,
    // not a parallel naming scheme. If these ever disagree, one of the two
    // surfaces is lying about where a setting lives.
    //
    // The walked form uses a literal cell path, which is what an author writes.
    // A string variable would not do: `get $key` binds a one-member cell path
    // whose single member is the whole string, dots included, so it looks for a
    // column literally named "supervisor.cpu_warn_fraction" and fails. Splitting
    // a string on dots is the source parser's job, not `get`'s.
    let env = s.run(
        json!({}),
        json!({"agree": "bool", "value": "float"}),
        json!({}),
        "let direct = (grimm get_config \"supervisor.cpu_warn_fraction\")\n\
         let walked = (grimm get_config_all | get supervisor.cpu_warn_fraction)\n\
         { agree: ($direct == $walked), value: $direct }",
    );
    assert!(!has_error(&env), "got {env}");
    assert_eq!(env["result"]["agree"].as_bool(), Some(true), "got {env}");
    assert_eq!(
        env["result"]["value"].as_f64(),
        Some(0.80),
        "the embedded default; got {env}",
    );
}

#[test]
fn an_unknown_key_errors_and_names_the_valid_ones() {
    let s = TestServer::new();
    let env = s.run(
        json!({}),
        json!({"out": "int"}),
        json!({}),
        "grimm get_config \"supervisor.nonesuch\"\n{ out: 0 }",
    );
    // An error rather than a null: a typo and a genuinely-null setting are
    // indistinguishable at the call site, and only one is a bug worth hearing about.
    assert!(has_error(&env), "an unknown key must error; got {env}");
    let text = error_text(&env);
    assert!(text.contains("not a config key"), "got {text}");
    assert!(
        text.contains("supervisor.vram_warn_headroom_mib"),
        "the error should carry the vocabulary; got {text}",
    );
}

#[test]
fn an_unpinned_port_reads_as_null_rather_than_a_sentinel() {
    let s = TestServer::new();
    let env = s.run(
        json!({}),
        json!({"is_null": "bool"}),
        json!({}),
        "{ is_null: ((grimm get_config \"channel.port\") == null) }",
    );
    assert_eq!(
        env["result"]["is_null"].as_bool(),
        Some(true),
        "an absent port is nothing, never a 0 that reads as a real port; got {env}",
    );
}

#[test]
fn a_pin_changes_what_the_config_surface_reports() {
    let s = TestServer::new();
    clear_config_pins();
    let env = s.run(
        json!({"pid": "int"}),
        json!({"before": "float", "after": "float"}),
        json!({"pid": me()}),
        "let key = \"supervisor.cpu_warn_fraction\"\n\
         let before = (grimm get_config $key)\n\
         grimm pin_config $key $args.pid 0.25\n\
         { before: $before, after: (grimm get_config $key) }",
    );
    assert!(!has_error(&env), "pin_config should run; got {env}");
    assert_eq!(env["result"]["before"].as_f64(), Some(0.80), "got {env}");
    assert_eq!(
        env["result"]["after"].as_f64(),
        Some(0.25),
        "a pin IS the setting - every reader goes through the same layer; got {env}",
    );
    clear_config_pins();
}

#[test]
fn the_motivating_case_is_zero_vram_headroom() {
    let s = TestServer::new();
    clear_config_pins();
    // Pin the headroom to 0 for a training run: the gate fires at
    // `total - headroom`, so zero puts the line at the card's full capacity and
    // usage never reaches it.
    let env = s.run(
        json!({"pid": "int"}),
        json!({"headroom": "int"}),
        json!({"pid": me()}),
        "grimm pin_config \"supervisor.vram_warn_headroom_mib\" $args.pid 0\n\
         { headroom: (grimm get_config \"supervisor.vram_warn_headroom_mib\") }",
    );
    assert!(!has_error(&env), "zero headroom must be legal; got {env}");
    assert_eq!(env["result"]["headroom"].as_i64(), Some(0), "got {env}");
    clear_config_pins();
}

#[test]
fn last_write_wins_across_calls() {
    let s = TestServer::new();
    clear_config_pins();
    for value in ["0.5", "0.9"] {
        let env = s.run(
            json!({"pid": "int"}),
            json!({"ok": "bool"}),
            json!({"pid": me()}),
            &format!(
                "grimm pin_config \"supervisor.disk_warn_fraction\" $args.pid {value}\n\
                 {{ ok: true }}",
            ),
        );
        assert!(!has_error(&env), "pin {value} should run; got {env}");
    }
    let env = s.run(
        json!({}),
        json!({"value": "float"}),
        json!({}),
        "{ value: (grimm get_config \"supervisor.disk_warn_fraction\") }",
    );
    assert_eq!(
        env["result"]["value"].as_f64(),
        Some(0.9),
        "a pin on a key replaces the one before it; got {env}",
    );
    clear_config_pins();
}

#[test]
fn only_the_supervisor_lines_are_pinnable() {
    let s = TestServer::new();
    clear_config_pins();
    // config_channel stays the sole mutator of the channel settings; a value with
    // two mutators is one whose effective setting depends on which you ask.
    for key in ["channel.spam_warn_rate", "channel.port", "channel.cert_dir"] {
        let env = s.run(
            json!({"pid": "int"}),
            json!({"out": "int"}),
            json!({"pid": me()}),
            &format!("grimm pin_config \"{key}\" $args.pid 1\n{{ out: 0 }}"),
        );
        assert!(has_error(&env), "`{key}` must not be pinnable; got {env}");
        assert!(
            error_text(&env).contains("not pinnable"),
            "got {}",
            error_text(&env),
        );
    }
    clear_config_pins();
}

#[test]
fn a_pin_is_held_to_the_config_files_own_bounds() {
    let s = TestServer::new();
    clear_config_pins();
    // A fraction outside (0, 1] would warn always or never. Sharing the file
    // layer's validator is what stops a pin reaching a state a config load would
    // have refused.
    for value in ["0.0", "1.5", "-0.2"] {
        let env = s.run(
            json!({"pid": "int"}),
            json!({"out": "int"}),
            json!({"pid": me()}),
            &format!(
                "grimm pin_config \"supervisor.ram_warn_fraction\" $args.pid {value}\n\
                 {{ out: 0 }}",
            ),
        );
        assert!(has_error(&env), "fraction {value} must be refused; got {env}");
    }
    clear_config_pins();
}

#[test]
fn a_pin_against_a_dead_process_is_refused() {
    let s = TestServer::new();
    clear_config_pins();
    // pid 0 is never a real process. Refusing at pin time rather than reaping a
    // tick later matters because the two look identical a second afterwards.
    let env = s.run(
        json!({}),
        json!({"out": "int"}),
        json!({}),
        "grimm pin_config \"supervisor.cpu_warn_fraction\" 0 0.5\n{ out: 0 }",
    );
    assert!(has_error(&env), "got {env}");
    assert!(
        error_text(&env).contains("not running"),
        "the refusal should say why; got {}",
        error_text(&env),
    );
    let after = s.run(
        json!({}),
        json!({"value": "float"}),
        json!({}),
        "{ value: (grimm get_config \"supervisor.cpu_warn_fraction\") }",
    );
    assert_eq!(
        after["result"]["value"].as_f64(),
        Some(0.80),
        "a refused pin must leave nothing behind; got {after}",
    );
    clear_config_pins();
}

#[test]
fn the_config_decls_reach_the_interact_lane_too() {
    let s = TestServer::new();
    let env = s.interact(
        json!({}),
        json!({"tables": "int"}),
        json!({}),
        "{ tables: (grimm get_config_all | columns | length) }",
    );
    assert_eq!(
        env["result"]["tables"].as_i64(),
        Some(2),
        "the whole grimm family registers per eval, on both lanes; got {env}",
    );
}
