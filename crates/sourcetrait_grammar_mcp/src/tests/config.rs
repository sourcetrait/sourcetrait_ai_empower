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
    // rather than something that surfaces much later at first use. Only variables
    // off the XDG / XDGX families are unresolvable; those families fall back.
    let err = from_text("[channel]\ncert_dir = \"$GRAMMAR_TEST_UNSET_VAR/certs\"\n")
        .expect_err("an unset off-family variable must fail the load");
    assert!(err.contains("GRAMMAR_TEST_UNSET_VAR"), "got {err}");
}

#[test]
fn the_xdg_and_xdgx_families_carry_spec_defaults() {
    // The expansion context falls back to these when a variable is unset, so the
    // embedded default cert_dir resolves on a machine without the SourceTrait
    // environment. Hardcoded here for now; sourcetrait_common's XdgDir does not
    // export its defaults and carries no XDGX family. XDGX builds on the xdg /
    // typical unix defaults; the .sys convention applies only under the dotsys
    // base spec. spec_default_for is the pure core, so both specs are driven
    // explicitly and nothing here reads the box's own environment.
    let home = PathBuf::from(std::env::var("HOME").expect("HOME set"));
    for (var, home_relative) in [
        ("XDG_CACHE_HOME", ".cache"),
        ("XDG_CONFIG_HOME", ".config"),
        ("XDG_DATA_HOME", ".local/share"),
        ("XDG_STATE_HOME", ".local/state"),
        ("XDGX_ASSET_HOME", ".local/share"),
        ("XDGX_EXECUTE_HOME", ".local/bin"),
        ("XDGX_LIBRARY_HOME", ".local/lib"),
        ("XDGX_PACKAGE_HOME", ".local/pkg"),
        ("XDGX_SECRET_DATA_HOME", ".secret/data"),
        ("XDGX_TMP_HOME", "tmp"),
    ] {
        assert_eq!(
            crate::config::spec_default_for(var, "xdg"),
            Some(home.join(home_relative)),
            "{var} must default under $HOME on the xdg base spec",
        );
    }
    for (var, home_relative) in [
        ("XDGX_ASSET_HOME", ".sys/local/share"),
        ("XDGX_EXECUTE_HOME", ".sys/local/bin"),
        ("XDGX_LIBRARY_HOME", ".sys/local/lib"),
        ("XDGX_PACKAGE_HOME", ".sys/local/pkg"),
        ("XDGX_SECRET_DATA_HOME", ".sys/.xdg/secret/data"),
    ] {
        assert_eq!(
            crate::config::spec_default_for(var, "dotsys"),
            Some(home.join(home_relative)),
            "{var} must move to the .sys convention under dotsys",
        );
    }
    assert_eq!(
        crate::config::spec_default_for("XDGX_TMP_HOME", "dotsys"),
        Some(home.join("tmp")),
        "the tmp home is home-relative on both specs",
    );
    assert_eq!(
        crate::config::spec_default_for("XDGX_BASE_SPEC", "xdg"),
        Some(PathBuf::from("xdg")),
        "the base-spec selector itself defaults to xdg",
    );
    for spec in ["xdg", "dotsys"] {
        assert!(
            crate::config::spec_default_for("XDGX_SHM_DIR", spec)
                .expect("XDGX_SHM_DIR has a default")
                .starts_with("/dev/shm"),
            "the shm default is user-keyed under /dev/shm on both specs",
        );
    }
    assert_eq!(
        crate::config::spec_default_for("GRAMMAR_TEST_UNSET_VAR", "xdg"),
        None,
        "an off-family variable has no default and stays a load error",
    );
}

/// Parse a `remotes.toml` body and resolve it, as the startup load of
/// `.grammar/mcp/remotes.toml` would.
fn remotes(
    text: &str,
) -> Result<std::collections::HashMap<String, crate::config::RemoteConfig>, String> {
    let file: crate::config::RemotesConfigToml = toml::from_str(text).map_err(|e| e.to_string())?;
    Ok(crate::config::RemotesConfig::try_from(file)?.by_alias)
}

#[test]
fn a_remote_entry_with_listen_is_a_listener() {
    let text = "\
[[remote]]
alias = \"bob\"
listen = \"127.0.0.1:9000\"
address = \"127.0.0.1\"
self_public_key_file = \"/keys/self.pem\"
self_private_key_file = \"/keys/self.key.pem\"
public_key_file = \"/keys/bob.pem\"
";
    let out = remotes(text).expect("valid listener entry");
    let entry = out.get("bob").expect("bob present");
    match &entry.role {
        crate::config::RemoteRole::Listener { bind, allow } => {
            assert_eq!(bind.to_string(), "127.0.0.1:9000");
            // `address` on a listener is a bare source-IP filter.
            assert_eq!(allow.map(|ip| ip.to_string()), Some("127.0.0.1".to_string()));
        }
        other => panic!("expected a listener, got {other:?}"),
    }
    assert!(entry.peer_public_key_file.ends_with("bob.pem"));
}

#[test]
fn a_listener_address_with_a_port_is_rejected() {
    // A listener's `address` is a bare source-IP filter; the host:port form is the
    // connector's dial-target format and must not be accepted here.
    let text = "\
[[remote]]
alias = \"bob\"
listen = \"127.0.0.1:9000\"
address = \"127.0.0.1:5555\"
self_public_key_file = \"/keys/self.pem\"
self_private_key_file = \"/keys/self.key.pem\"
public_key_file = \"/keys/bob.pem\"
";
    let err = remotes(text).expect_err("a host:port listener address must fail");
    assert!(err.contains("bare IP"), "got {err}");
}

#[test]
fn a_remote_entry_without_listen_is_a_connector() {
    let text = "\
[[remote]]
alias = \"bob\"
address = \"127.0.0.1:9001\"
self_public_key_file = \"/keys/self.pem\"
self_private_key_file = \"/keys/self.key.pem\"
public_key_file = \"/keys/bob.pem\"
";
    let out = remotes(text).expect("valid connector entry");
    match &out.get("bob").expect("bob present").role {
        crate::config::RemoteRole::Connector { addr } => {
            assert_eq!(addr.to_string(), "127.0.0.1:9001");
        }
        other => panic!("expected a connector, got {other:?}"),
    }
}

#[test]
fn a_connector_without_an_address_is_rejected() {
    let text = "\
[[remote]]
alias = \"bob\"
self_public_key_file = \"/keys/self.pem\"
self_private_key_file = \"/keys/self.key.pem\"
public_key_file = \"/keys/bob.pem\"
";
    let err = remotes(text).expect_err("a connector needs an address");
    assert!(err.contains("address"), "got {err}");
}

#[test]
fn a_remote_entry_missing_a_key_is_rejected() {
    let text = "[[remote]]\nalias = \"bob\"\naddress = \"127.0.0.1:9001\"\n";
    let err = remotes(text).expect_err("a missing key must fail");
    assert!(err.contains("bob"), "got {err}");
}

#[test]
fn a_typoed_remote_field_is_rejected() {
    // deny_unknown_fields on the entry catches a field typo loudly.
    let text = "[[remote]]\nalias = \"bob\"\naddres = \"127.0.0.1:9001\"\n";
    assert!(remotes(text).is_err(), "a typo'd field must be rejected");
}

#[test]
fn a_bad_remote_address_fails_the_load() {
    let text = "\
[[remote]]
alias = \"bob\"
address = \"not-an-address\"
self_public_key_file = \"/keys/self.pem\"
self_private_key_file = \"/keys/self.key.pem\"
public_key_file = \"/keys/bob.pem\"
";
    let err = remotes(text).expect_err("a non ip:port address must fail");
    assert!(err.contains("is not ip:port"), "got {err}");
}
