use std::path::Path;

use sourcetrait_grammar_mcp::guts::{
    TestServer, has_error, library_block, valid_function_source, write_source,
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
        env.get("libraries").is_none(),
        "the structured hierarchy is gone; `signatures` replaces it. got {env}",
    );
    assert!(env["signatures"].is_string(), "signatures is a text block; got {env}");
}

#[test]
#[named]
fn info_renders_a_committed_library_as_a_signature_block() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("treelib");
    let est = s.library("new", "sourcetrait/treelib", src.to_str().unwrap());
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
    let actual = library_block(&block, "treelib");
    assert_eq!(
        actual,
        golden_text("info_treelib.txt", &actual),
        "the treelib signature block drifted from the golden",
    );
    assert!(
        block.contains("sourcetrait/\n"),
        "the author heads its own group and carries no summary; got:\n{block}",
    );
}

#[test]
#[named]
fn the_indentation_is_the_hierarchy() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("implib");
    let est = s.library("new", "sourcetrait/implib", src.to_str().unwrap());
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
        library_block(&signatures(&s), "implib"),
        " implib:\n  math:\n   double <x:int> <out:int>\n",
        "one space per level, and an undocumented node carries no ` # `",
    );
}

#[test]
#[named]
fn summaries_ride_the_line_and_are_omitted_when_absent() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("doctreelib");
    let _ = s.library("new", "sourcetrait/doctreelib", src.to_str().unwrap());
    write_source(&src, "mod.nu", "# the doctree library\nexport module m\n");
    write_source(&src, "m/mod.nu", "# the m module\nexport use fn\n");
    write_source(
        &src,
        "m/fn/mod.nu",
        "# the fn summary\nexport def main [args: record<x: int>]: nothing -> record<out: int> { { out: $args.x } }\n",
    );
    let committed = s.commit("sourcetrait/doctreelib");
    assert!(!has_error(&committed), "commit failed: {committed}");

    assert_eq!(
        library_block(&signatures(&s), "doctreelib"),
        " doctreelib: # the doctree library\n  m: # the m module\n   fn <x:int> <out:int> # the fn summary\n",
        "summary is part of the line, on every kind that has one",
    );
}

#[test]
#[named]
fn a_void_renders_as_empty_angles() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("voidlib");
    let _ = s.library("new", "sourcetrait/voidlib", src.to_str().unwrap());
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
        library_block(&signatures(&s), "voidlib"),
        " voidlib:\n  m:\n   ping <> <>\n",
        "a void arg list and a void result each render `<>`, never `<nothing>`",
    );
}

#[test]
#[named]
fn an_author_heads_its_group_exactly_once() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    for leaf in ["onelib", "twolib"] {
        let src = t.temp_dir().join(leaf);
        let name = format!("sourcetrait/{leaf}");
        let _ = s.library("new", &name, src.to_str().unwrap());
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
    let heads = block.lines().filter(|l| *l == "sourcetrait/").count();
    assert_eq!(
        heads, 1,
        "the author line is emitted once per GROUP, not once per library; got:\n{block}",
    );
}
