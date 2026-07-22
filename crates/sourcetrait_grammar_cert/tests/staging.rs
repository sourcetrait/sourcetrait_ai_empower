//! Staging cleanup: `install` consumes its staging dir on success, but only when that
//! dir holds exactly what we put there. A blind `remove_dir_all` on a path that came
//! from an argument is the deletion mirror of blindly creating a system path.
//!
//! Driven through `guts` rather than the binary: `install` elevates through `sudo`,
//! which needs an interactive password no test can supply.

use sourcetrait_grammar_cert::guts;
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

fn seed(dir: &std::path::Path, name: &str) {
    std::fs::create_dir_all(dir).expect("mkdir staging");
    for file in guts::artifact_names(name) {
        std::fs::write(dir.join(file), "x").expect("write artifact");
    }
}

#[tested]
fn removes_a_staging_dir_holding_only_our_artifacts() {
    let t = testing::test!({ .using_temp_dir() });
    let dir = t.temp_dir().join("certs");
    seed(&dir, "grammar");

    guts::consume_staging(&dir, "grammar").expect("a clean staging dir is removed");
    assert!(!dir.exists(), "staging should be gone, since it held a CA private key");
}

/// The case that matters: something else lives there, so we must not delete it.
#[tested]
fn keeps_a_staging_dir_holding_anything_else() {
    let t = testing::test!({ .using_temp_dir() });
    let dir = t.temp_dir().join("certs");
    seed(&dir, "grammar");
    std::fs::write(dir.join("README.txt"), "notes").expect("write stray");

    let reason = guts::consume_staging(&dir, "grammar").expect_err("must refuse to delete");
    assert!(reason.contains("README.txt"), "the reason should name it; got {reason:?}");
    assert!(dir.exists(), "the dir must survive");
    assert!(dir.join("README.txt").exists(), "the stray file must survive");
    assert!(
        dir.join("authority_grammar.key.pem").exists(),
        "our own artifacts must survive too - a partial delete is worse than none",
    );
}

/// A subdirectory counts as unexpected: `remove_dir` after removing the known files is
/// the second net, so nothing recurses.
#[tested]
fn keeps_a_staging_dir_holding_a_subdirectory() {
    let t = testing::test!({ .using_temp_dir() });
    let dir = t.temp_dir().join("certs");
    seed(&dir, "grammar");
    std::fs::create_dir(dir.join("nested")).expect("mkdir nested");

    let reason = guts::consume_staging(&dir, "grammar").expect_err("must refuse to delete");
    assert!(reason.contains("nested"), "got {reason:?}");
    assert!(dir.join("nested").exists());
}

#[tested]
fn reports_a_missing_staging_dir_rather_than_panicking() {
    let t = testing::test!({ .using_temp_dir() });
    let dir = t.temp_dir().join("absent");
    assert!(guts::consume_staging(&dir, "grammar").is_err());
}
