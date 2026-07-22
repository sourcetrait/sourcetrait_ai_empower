use std::path::Path;

use sourcetrait_grammar_mcp::guts::{
    TestServer, has_error, rig_block, valid_function_source, write_source,
};
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

/// Read a committed golden under `<crate>/testing/goldens/`, comparing it to
/// `actual`. `BLESS=1 cargo test` (re)writes it from the actual value; without
/// the golden present (and no BLESS) the read fails loudly.
///
/// The signature block is WHITESPACE-SIGNIFICANT, which is what makes it a good
/// golden: an indentation bug shows up as a plain file diff rather than as a
/// structural assertion nobody reads.
fn golden_text(
    name: &str,
    actual: &str,
) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testing/goldens")
        .join(name);
    if std::env::var("BLESS").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir goldens");
        std::fs::write(&path, actual).expect("write golden");
    }
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read golden {name}: {e} (run with BLESS=1 to (re)generate)"))
}

fn signatures(s: &TestServer) -> String {
    s.info()["signatures"]
        .as_str()
        .expect("info carries a signatures block")
        .to_string()
}

#[test]
fn info_returns_static_server_state() {
    let s = TestServer::new();
    let env = s.info();
    assert_eq!(env["name"].as_str(), Some("grammar"), "name should be grammar; got {env}");

    let version = env["version"].as_str().expect("version field");
    assert_eq!(
        version,
        env!("CARGO_PKG_VERSION"),
        "version should match crate CARGO_PKG_VERSION",
    );

    let nu_version = env["nu_version"].as_str().expect("nu_version field");
    assert!(
        !nu_version.is_empty() && nu_version.contains('.'),
        "nu_version should be non-empty + semver-shaped; got {nu_version:?}",
    );

    let plugins = env["plugins"].as_array().expect("plugins should be an array");
    for p in plugins {
        let entry = p
            .as_array()
            .expect("each plugin is a positional [name, version] pair");
        assert_eq!(entry.len(), 2, "plugin entry is a 2-element [name, version]; got {entry:?}");
        let name = entry[0].as_str().expect("plugin name is a string");
        assert!(!name.is_empty(), "plugin name should be non-empty");
        assert!(
            entry[1].is_string() || entry[1].is_null(),
            "plugin version slot should be string-or-null; got {:?}",
            entry[1],
        );
    }

    assert!(
        env.get("rigs").is_none(),
        "the structured hierarchy is gone; `signatures` replaces it. got {env}",
    );
    assert!(env["signatures"].is_string(), "signatures is a text block; got {env}");
}

#[tested]
fn info_renders_a_committed_rig_as_a_signature_block() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("treelib");
    let est = s.rig("new", "sourcetrait/treelib", src.to_str().unwrap());
    assert!(!has_error(&est), "establish failed: {est}");
    for (module_path, name, args_inner, result_inner, body) in [
        ("solo", "rootfn", "x: int", "out: int", "{ out: ($args.x + 1) }"),
        (
            "alpha",
            "a1",
            "s: string",
            "len: int",
            "{ len: ($args.s | str length) }",
        ),
        (
            "alpha/beta",
            "b1",
            "x: int, t: record<y: string, n: list<int>>",
            "out: record<y: string>",
            "{ out: { y: $args.t.y } }",
        ),
    ] {
        let np = format!("sourcetrait/treelib:{module_path}:{name}");
        let scaffold = s.scaffold(&[np.as_str()]);
        assert!(!has_error(&scaffold), "scaffold {np} failed: {scaffold}");
        let rel = format!("{module_path}/{name}/mod.nu");
        write_source(&src, &rel, &valid_function_source(args_inner, result_inner, body));
    }
    let committed = s.commit("sourcetrait/treelib");
    assert!(!has_error(&committed), "commit failed: {committed}");

    let block = signatures(&s);
    let actual = rig_block(&block, "treelib");
    assert_eq!(
        actual,
        golden_text("info_treelib.txt", &actual),
        "the treelib signature block drifted from the golden",
    );
    assert!(
        block.contains("sourcetrait\n"),
        "the author heads its own group and carries no summary; got:\n{block}",
    );
}

#[tested]
fn the_indentation_is_the_hierarchy() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("implib");
    let est = s.rig("new", "sourcetrait/implib", src.to_str().unwrap());
    assert!(!has_error(&est), "establish failed: {est}");
    write_source(&src, "mod.nu", "export module math\n");
    write_source(&src, "math/mod.nu", "export use double\n");
    write_source(
        &src,
        "math/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
    let committed = s.commit("sourcetrait/implib");
    assert!(!has_error(&committed), "commit failed: {committed}");

    assert_eq!(
        rig_block(&signatures(&s), "implib"),
        " implib\n  math\n   double <x:int> <out:int>\n",
        "one space per level, and an undocumented line carries no ` # `",
    );
}

#[tested]
fn summaries_ride_the_line_and_are_omitted_when_absent() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("doctreelib");
    let _ = s.rig("new", "sourcetrait/doctreelib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "# the doctree rig\nexport module m\n");
    write_source(&src, "m/mod.nu", "# the m module\nexport use fn\n");
    write_source(
        &src,
        "m/fn/mod.nu",
        "# the fn summary\nexport def main [args: record<x: int>]: nothing -> record<out: int> { { out: $args.x } }\n",
    );
    let committed = s.commit("sourcetrait/doctreelib");
    assert!(!has_error(&committed), "commit failed: {committed}");

    assert_eq!(
        rig_block(&signatures(&s), "doctreelib"),
        " doctreelib # the doctree rig\n  m # the m module\n   fn <x:int> <out:int> # the fn summary\n",
        "summary is part of the line, on every kind that has one",
    );
}

#[tested]
fn a_void_renders_as_empty_angles() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("voidlib");
    let _ = s.rig("new", "sourcetrait/voidlib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use ping\n");
    write_source(
        &src,
        "m/ping/mod.nu",
        "export def main [args: nothing]: nothing -> nothing { }\n",
    );
    let committed = s.commit("sourcetrait/voidlib");
    assert!(!has_error(&committed), "commit failed: {committed}");

    assert_eq!(
        rig_block(&signatures(&s), "voidlib"),
        " voidlib\n  m\n   ping <> <>\n",
        "a void arg list and a void result each render `<>`, never `<nothing>`",
    );
}

#[tested]
fn a_multi_line_summary_is_flattened_onto_its_line() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("wraplib");
    let _ = s.rig("new", "sourcetrait/wraplib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export use go\n");
    // A doc comment whose SUMMARY - everything before the first blank line -
    // spans two source lines. The parser joins those with a newline, and real
    // rigs in the namespace are written this way.
    write_source(
        &src,
        "m/go/mod.nu",
        "# Runs the thing against the other thing,\n# gating each step as it goes.\nexport def main [args: record<x: int>]: nothing -> record<out: int> { { out: $args.x } }\n",
    );
    let committed = s.commit("sourcetrait/wraplib");
    assert!(!has_error(&committed), "commit failed: {committed}");

    let block = rig_block(&signatures(&s), "wraplib");
    assert_eq!(
        block,
        " wraplib\n  m\n   go <x:int> <out:int> # Runs the thing against the other thing, gating each step as it goes.\n",
        "a two-line summary must be FLATTENED onto its own line",
    );
    assert_eq!(
        block.lines().count(),
        3,
        "three signatures means three lines - an unflattened summary would add a \
         fourth at column 0, which the grammar reads as an AUTHOR; got:\n{block}",
    );
}

#[tested]
fn a_mixed_module_states_its_own_call_separator() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("mixedlib");
    let _ = s.rig("new", "sourcetrait/mixedlib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "export module m\n");
    // `m` holds BOTH a submodule and a direct call. This is the case that sank
    // the trailing-character format - no single character describes it - and it
    // needs NOTHING special here, because a call is recognizable by its own
    // shape rather than by what its parent announced.
    write_source(&src, "m/mod.nu", "export module deep\nexport use here\n");
    write_source(
        &src,
        "m/here/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    write_source(&src, "m/deep/mod.nu", "export use down\n");
    write_source(
        &src,
        "m/deep/down/mod.nu",
        &valid_function_source("y: int", "out: int", "{ out: $args.y }"),
    );
    let committed = s.commit("sourcetrait/mixedlib");
    assert!(!has_error(&committed), "commit failed: {committed}");

    assert_eq!(
        rig_block(&signatures(&s), "mixedlib"),
        " mixedlib\n  m\n   here <x:int> <out:int>\n   deep\n    down <y:int> <out:int>\n",
        "a mixed module needs no marker: `here` is a call because it CARRIES the \
         two signature groups, so a reader knows the module path ended at `m` \
         and reads sourcetrait/mixedlib:m:here, while `deep` continues it",
    );
}

#[tested]
fn an_author_heads_its_group_exactly_once() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    for leaf in ["onelib", "twolib"] {
        let src = t.temp_dir().join(leaf);
        let name = format!("sourcetrait/{leaf}");
        let _ = s.rig("new", &name, src.to_str().unwrap());
        write_source(&src, "mod.nu", "export module m\n");
        write_source(&src, "m/mod.nu", "export use go\n");
        write_source(
            &src,
            "m/go/mod.nu",
            &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
        );
        let committed = s.commit(&name);
        assert!(!has_error(&committed), "commit {name} failed: {committed}");
    }
    let block = signatures(&s);
    let heads = block.lines().filter(|l| *l == "sourcetrait").count();
    assert_eq!(
        heads, 1,
        "the author line is emitted once per GROUP, not once per rig; got:\n{block}",
    );
}
