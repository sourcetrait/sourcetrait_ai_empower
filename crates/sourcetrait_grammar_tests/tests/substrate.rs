use sourcetrait_grammar_tests::*;
use sourcetrait_common::testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

/// A fresh host process creates its namespace substrate at startup: the signing
/// keypair + the rigs git repo (HEAD on `main`). This is a first-startup /
/// separate-process concern, hence a SYSTEM test.
#[tested]
fn substrate_initializes_on_first_startup() {
    let t = testing::test!({ .using_temp_dir() });
    let host = Host::spawn_args(t.temp_dir(), &["--id", "sid"]);
    let namespace = namespace_dir(host.data_home(), "sid", "default");

    let keypair = namespace.join("keypair");
    assert!(
        keypair.join("id_grammar").exists(),
        "private key should exist at {}",
        keypair.join("id_grammar").display(),
    );
    assert!(keypair.join("id_grammar.pub").exists(), "public key should exist");
    assert!(keypair.join("allowed_signers").exists(), "allowed_signers should exist");

    let libs = namespace.join("rigs");
    assert!(libs.join(".git").exists(), ".git should exist in rigs");
    let head = std::fs::read_to_string(libs.join(".git").join("HEAD")).expect("read HEAD");
    assert!(
        head.contains("refs/heads/main"),
        "HEAD should point at main; got {head:?}",
    );
}
