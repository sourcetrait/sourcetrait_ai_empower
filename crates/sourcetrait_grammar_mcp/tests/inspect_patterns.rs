//! Pattern-targeted `inspect()` - the `{signatures}` member of the doc oneof.
//!
//! Its OWN test binary on purpose: the in-process store is shared per binary, so
//! here it holds only what this file commits, which is what makes the
//! store-wide patterns (`*`, `author/`) assertable as exact blocks rather than
//! as `contains` checks that another test's library could satisfy.

use std::path::Path;

use serde_json::json;
use sourcetrait_grammar_mcp::guts::{
    TestServer, has_error, valid_function_source, write_source,
};
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

/// A library shaped to exercise every pattern form at once:
///
/// ```txt
/// sourcetrait
///  patlib # the patlib library
///   m # the m module
///    here <x:int> <out:int>
///    deep
///     down <y:int> <out:int>
///   other
///    solo <n:int> <out:int>
/// ```
///
/// `m` is MIXED - a direct call beside a submodule - which is the case the
/// format's shape rule exists for; `other` gives `ModuleTree` something to
/// exclude; and `other` / `deep` carry no doc, so the omit-the-summary rule is
/// covered in the same fixture.
fn commit_patlib(
    s: &TestServer,
    src: &Path,
) {
    let _ = s.library("new", "sourcetrait/patlib", src.to_str().unwrap());
    write_source(src, "mod.nu", "# the patlib library\nexport module m\nexport module other\n");
    write_source(src, "m/mod.nu", "# the m module\nexport module deep\nexport module here\nexport use here\n");
    write_source(
        src,
        "m/here/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    write_source(src, "m/deep/mod.nu", "export module down\nexport use down\n");
    write_source(
        src,
        "m/deep/down/mod.nu",
        &valid_function_source("y: int", "out: int", "{ out: $args.y }"),
    );
    write_source(src, "other/mod.nu", "export module solo\nexport use solo\n");
    write_source(
        src,
        "other/solo/mod.nu",
        &valid_function_source("n: int", "out: int", "{ out: $args.n }"),
    );
    let committed = s.commit("sourcetrait/patlib");
    assert!(!has_error(&committed), "commit failed: {committed}");
}

fn signatures(
    s: &TestServer,
    namepath: &str,
) -> String {
    let env = s.inspect(namepath);
    assert!(!has_error(&env), "inspect {namepath} errored: {env}");
    assert_eq!(
        env.as_object().expect("envelope object").len(),
        1,
        "the envelope stays EXACTLY one field for a pattern too; got {env}",
    );
    env["doc"]["signatures"]
        .as_str()
        .unwrap_or_else(|| panic!("inspect {namepath} returned no signatures member: {env}"))
        .to_string()
}

const WHOLE: &str = "sourcetrait\n patlib # the patlib library\n  m # the m module\n   here <x:int> <out:int>\n   deep\n    down <y:int> <out:int>\n  other\n   solo <n:int> <out:int>\n";

#[test]
#[named]
fn the_store_wide_patterns_render_the_whole_block() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    commit_patlib(&s, &t.temp_dir().join("patlib"));

    // `*` and the only author present must both render exactly what info()
    // does, since this store holds one library.
    assert_eq!(signatures(&s, "*"), WHOLE);
    assert_eq!(signatures(&s, "sourcetrait/"), WHOLE);
    assert_eq!(
        signatures(&s, "*"),
        s.info()["signatures"].as_str().expect("info signatures"),
        "a `*` inspect and info() are the SAME renderer and must not drift",
    );
}

#[test]
#[named]
fn a_library_pattern_renders_that_library() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    commit_patlib(&s, &t.temp_dir().join("patlib"));
    assert_eq!(signatures(&s, "sourcetrait/patlib:"), WHOLE);
}

#[test]
#[named]
fn a_module_tree_pattern_descends_and_excludes_its_siblings() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    commit_patlib(&s, &t.temp_dir().join("patlib"));

    assert_eq!(
        signatures(&s, "sourcetrait/patlib:m/"),
        "sourcetrait\n patlib # the patlib library\n  m # the m module\n   here <x:int> <out:int>\n   deep\n    down <y:int> <out:int>\n",
        "`/` descends the whole subtree under m, and `other` is not in it",
    );
    assert_eq!(
        signatures(&s, "sourcetrait/patlib:m/deep/"),
        "sourcetrait\n patlib # the patlib library\n  m # the m module\n   deep\n    down <y:int> <out:int>\n",
        "a deeper root still emits every ANCESTOR line, so the indentation \
         spells sourcetrait/patlib:m/deep:down and the block stays readable",
    );
}

#[test]
#[named]
fn a_module_calls_pattern_selects_calls_without_descending() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    commit_patlib(&s, &t.temp_dir().join("patlib"));

    assert_eq!(
        signatures(&s, "sourcetrait/patlib:m:"),
        "sourcetrait\n patlib # the patlib library\n  m # the m module\n   here <x:int> <out:int>\n",
        "`:` selects the CALL level: `here` is in, the `deep` submodule is not",
    );
}

#[test]
#[named]
fn patterns_that_match_nothing_render_empty() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    commit_patlib(&s, &t.temp_dir().join("patlib"));

    // A filter matching nothing yields an empty block rather than an error -
    // the same answer a fresh namespace gives, and the same one the unresolved
    // `.` purview stub must give.
    assert_eq!(signatures(&s, "."), "", "the unresolved purview stub names nothing");
    assert_eq!(signatures(&s, "bob/"), "", "an author with no libraries here");
    assert_eq!(signatures(&s, "sourcetrait/ghostlib:"), "", "an absent library");
    assert_eq!(
        signatures(&s, "sourcetrait/patlib:nosuch/"),
        "",
        "a module the library does not have leaves NO orphan author or library \
         heading behind",
    );
}

#[test]
#[named]
fn exact_namepaths_are_unchanged_by_the_pattern_arm() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    commit_patlib(&s, &t.temp_dir().join("patlib"));

    let call = s.inspect("sourcetrait/patlib:m:here");
    assert_eq!(
        call["doc"]["signature"].as_str(),
        Some("sourcetrait/patlib:m:here <x:int> <out:int>"),
        "an exact call still returns the STANDALONE signature; got {call}",
    );
    assert!(
        call["doc"].get("signatures").is_none(),
        "an exact coordinate must not pick up the pattern member; got {call}",
    );

    let module = s.inspect("sourcetrait/patlib:m");
    assert_eq!(module["doc"]["summary"].as_str(), Some("the m module"));
    let library = s.inspect("sourcetrait/patlib");
    assert_eq!(library["doc"]["summary"].as_str(), Some("the patlib library"));

    // Shape classification did not loosen the EXACT parser: a bare author names
    // no addressable coordinate and still errors.
    assert!(has_error(&s.inspect("sourcetrait")), "a bare author is not a coordinate");
}

#[test]
#[named]
fn a_pattern_is_never_callable() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    commit_patlib(&s, &t.temp_dir().join("patlib"));

    for pattern in ["*", ".", "sourcetrait/", "sourcetrait/patlib:", "sourcetrait/patlib:m:"] {
        let env = s.call(pattern, json!({"x": 1}));
        assert!(
            has_error(&env),
            "call({pattern}) must be refused - call takes an exact function \
             namepath only; got {env}",
        );
    }
}
