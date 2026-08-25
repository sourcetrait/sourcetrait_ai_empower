//! The `channel_*` tools where they can be driven WITHOUT a hub: the tools are
//! registered and reachable, an unopened channel refuses verification with a legible
//! kind, and closing nothing succeeds. `channel_open` itself binds a socket and
//! presents a CA-issued leaf, so it is exercised live on the test channel instead of
//! here, where it would only be asserting the box's certificate installation.

use sourcetrait_grammar_mcp::guts::{TestServer, error_kind, has_error};
use sourcetrait_common::testing::prelude::*;

/// One shared in-process server per test binary: constructing a TestServer runs
/// the namespace substrate (keypair, rigs repo git config), which must not race
/// itself across parallel tests.
static TESTING: testing::ModuleWith<TestServer> = testing::module_with!(Integration, {
    .setup(|_| TestServer::new())
});

#[test]
fn verifying_an_unopened_channel_errors_with_channel_not_open() {
    let s = TESTING.harness();
    let env = s.channel_verified();
    assert_eq!(
        error_kind(&env),
        Some("channel::not_open"),
        "there is nothing to verify until channel_open() runs; got {env}",
    );
}

#[test]
fn closing_an_unopened_channel_is_idempotent_success() {
    let s = TESTING.harness();
    let env = s.channel_close();
    assert!(!has_error(&env), "closing what is already closed is the requested state; got {env}");
    assert!(
        env.is_null(),
        "channel_close is a no-return tool, so success carries no payload; got {env}",
    );
    assert!(s.channel_close().is_null(), "and it stays a no-op on repeat");
}
