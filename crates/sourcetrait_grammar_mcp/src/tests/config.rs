use crate::*;

/// Build a runtime config from file text alone; the argument-owned values are
/// placeholders, since the file surface is what these lock.
fn from_text(text: &str) -> Result<Config, String> {
    let toml: ConfigToml = toml::from_str(text).map_err(|e| e.to_string())?;
    Config::from_toml(
        toml,
        "id".to_string(),
        "ns".to_string(),
        PathBuf::from("/w"),
        DenySet::default(),
        false,
    )
}

/// The embedded default, read through the same path rather than restated as a literal,
/// so these tests cannot drift from defaults/grammar_mcp.toml.
fn embedded_cert_dir() -> PathBuf {
    from_text("")
        .expect("embedded defaults parse")
        .channel
        .cert_dir
}

#[test]
fn an_empty_file_yields_the_embedded_defaults() {
    let config = from_text("").expect("empty file is valid");
    assert_eq!(
        config.channel.port, None,
        "an unpinned port must stay None, not become a 0 sentinel",
    );
    assert!(
        config.channel.cert_dir.is_absolute(),
        "the default cert_dir must be expanded at load, not left as a $VAR; got {}",
        config.channel.cert_dir.display(),
    );
    assert!(
        config.channel.cert_dir.ends_with("certs"),
        "got {}",
        config.channel.cert_dir.display(),
    );
}

#[test]
fn an_omitted_cert_dir_falls_back_to_the_default() {
    let config = from_text("[channel]\nport = 47800\n").expect("valid");
    assert_eq!(config.channel.port, Some(47800));
    assert_eq!(
        config.channel.cert_dir,
        embedded_cert_dir(),
        "omitting cert_dir must inherit the embedded default, not blank it",
    );
}

#[test]
fn an_explicit_cert_dir_wins_and_is_expanded() {
    let config = from_text("[channel]\ncert_dir = \"~/some/certs\"\n").expect("valid");
    assert!(
        config.channel.cert_dir.is_absolute(),
        "a `~` path must be expanded at load; got {}",
        config.channel.cert_dir.display(),
    );
    assert!(config.channel.cert_dir.ends_with("some/certs"));
    assert_eq!(config.channel.port, None, "port stays unpinned");
}

#[test]
fn a_privileged_or_zero_port_is_rejected() {
    // 0 is the syscall's "assign me one" convention, never a configurable value; below
    // 1024 needs root, which the host never has, so it could only fail at bind.
    for port in [0, 80, 443, 1023] {
        let err = from_text(&format!("[channel]\nport = {port}\n"))
            .expect_err(&format!("port {port} must be rejected"));
        assert!(err.contains("1024-65535"), "port {port}: got {err}");
    }
}

#[test]
fn the_lowest_unprivileged_port_is_accepted() {
    let config = from_text("[channel]\nport = 1024\n").expect("1024 is valid");
    assert_eq!(config.channel.port, Some(1024));
}

#[test]
fn the_namespace_cannot_be_set_from_a_file() {
    // The whole point of deny_unknown_fields here: a file must not be able to move the
    // namespace out from under the .mcp.json entry that launched the server.
    for text in [
        "id = \"other\"\n",
        "namespace = \"other\"\n",
        "work_dir = \"/elsewhere\"\n",
        "deny = [\"run\"]\n",
        // `--test` joins the argument-owned set for the same reason: it selects
        // the namespace, so a file-settable one could move the namespace out from
        // under the .mcp.json entry that launched the server.
        "test = true\n",
    ] {
        assert!(
            from_text(text).is_err(),
            "{text:?} must be rejected as an unknown field",
        );
    }
}

#[test]
fn the_test_flag_is_carried_on_the_runtime_config() {
    let toml: ConfigToml = toml::from_str("").expect("empty file is valid");
    let config = Config::from_toml(
        toml,
        "id".to_string(),
        TEST_NAMESPACE.to_string(),
        PathBuf::from("/w"),
        DenySet::default(),
        true,
    )
    .expect("valid");
    assert!(config.test, "the watchdog half reads this at every tick");
    assert!(
        !from_text("").expect("valid").test,
        "and an ordinary host is not a test host",
    );
}

#[test]
fn a_retired_channel_key_is_rejected() {
    // bind / cert_name / verify_timeout_secs were removed; a stale file must fail loudly
    // rather than be silently ignored.
    for text in [
        "[channel]\nbind = \"0.0.0.0\"\n",
        "[channel]\ncert_name = \"custom\"\n",
        "[channel]\nverify_timeout_secs = 42\n",
    ] {
        assert!(from_text(text).is_err(), "{text:?} must be rejected");
    }
}

#[test]
fn cert_paths_derive_both_leaf_and_key_from_the_dir() {
    let config = from_text("[channel]\ncert_dir = \"/somewhere/certs\"\n").expect("valid");
    let (leaf, key) = config.channel.cert_paths();
    assert!(leaf.ends_with("entity_grammar.pem"), "got {}", leaf.display());
    assert!(
        key.ends_with("entity_grammar.key.pem"),
        "got {}",
        key.display(),
    );
}

#[test]
fn an_unset_variable_fails_the_load() {
    // Expansion happens at the boundary, so an unresolvable path is a config error
    // rather than something that surfaces much later at first use.
    let err = from_text("[channel]\ncert_dir = \"$GRAMMAR_TEST_UNSET_VAR/certs\"\n")
        .expect_err("an unset variable must fail the load");
    assert!(err.contains("GRAMMAR_TEST_UNSET_VAR"), "got {err}");
}
