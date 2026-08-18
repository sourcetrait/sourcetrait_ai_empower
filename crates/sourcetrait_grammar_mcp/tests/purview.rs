//! Purviews: the namespace-level scoped view of the callable surface.
//!
//! Its OWN test binary, because a purview table is per NAMESPACE and the
//! in-process namespace is shared per binary - these tests mutate it, so they
//! must not share it with tests that assume a whole-namespace view.
//!
//! Each test resets the table first and uses unique rig and purview names, so
//! nothing here depends on the order cargo happens to run them in.

use std::path::Path;

use sourcetrait_grammar_mcp::guts::{
    TestServer, error_kind, has_error, rig_block, valid_function_source, write_source,
};
use sourcetrait_common::testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

/// Commit a one-call rig, so a purview has something concrete to include or
/// exclude.
fn commit_rig(
    s: &TestServer,
    src: &Path,
    name: &str,
) {
    let _ = s.rig("new", name, src.to_str().unwrap());
    write_source(src, "mod.nu", "export module m\n");
    write_source(src, "m/mod.nu", "export module go\nexport use go\n");
    write_source(
        src,
        "m/go/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    let committed = s.commit(name);
    assert!(!has_error(&committed), "commit {name} failed: {committed}");
}

/// The persisted table. The write tools no longer echo it, so this is HOW a
/// test checks what a write did to configuration.
fn table(s: &TestServer) -> Vec<(String, Vec<String>)> {
    let listed = s.purviews();
    listed["purviews"]
        .as_array()
        .expect("purviews")
        .iter()
        .map(|row| {
            let id = row[0].as_str().unwrap_or_default().to_string();
            let patterns = row[1]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            (id, patterns)
        })
        .collect()
}

fn patterns_of(
    s: &TestServer,
    id: &str,
) -> Option<Vec<String>> {
    table(s).into_iter().find(|(k, _)| k == id).map(|(_, v)| v)
}

/// Back to the startup state: `default` alone at `['*']`, nothing else in view.
fn reset_purviews(s: &TestServer) {
    for (id, _) in table(s) {
        let _ = s.purview_configure(&id, &[]);
    }
    let _ = s.purview(&[]);
}

fn signatures(env: &serde_json::Value) -> &str {
    env["signatures"].as_str().expect("signatures block")
}

#[tested]
fn a_fresh_namespace_starts_at_default_seeing_everything() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_rig(&s, &t.temp_dir().join("pvone"), "sourcetrait/pvone");

    let info = s.info();
    assert_eq!(info["purview"][0][0].as_str(), Some("default"));
    assert_eq!(
        info["purview"][0][1][0].as_str(),
        Some("*"),
        "startup MATERIALIZES `default` as `*` - there is no such thing as an \
         unconfigured default, so this is a row rather than a fallback; got {info}",
    );
    assert!(signatures(&info).contains("pvone"), "got {info}");
    assert!(
        patterns_of(&s, "default").is_some(),
        "and it is really PERSISTED, not synthesized on read",
    );
}

#[tested]
fn deleting_default_resets_it_rather_than_removing_it() {
    let _t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);

    let env = s.purview_configure("default", &[]);
    assert!(!has_error(&env), "configure failed: {env}");
    assert_eq!(
        patterns_of(&s, "default"),
        Some(vec!["*".to_string()]),
        "an empty list DELETES any other purview, but `default` has no \
         not-existing state - it resets to what startup would write",
    );
}

#[tested]
fn configure_writes_the_namespace_meta_file_and_reads_back() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_rig(&s, &t.temp_dir().join("pvtwo"), "sourcetrait/pvtwo");

    let env = s.purview_configure("iter/one", &["sourcetrait/pvtwo:"]);
    assert!(!has_error(&env), "configure failed: {env}");
    let path = s.purviews_path();
    assert!(
        path.ends_with("purviews.nuon")
            && path.parent().is_some_and(|p| p.ends_with(".meta"))
            && path.exists(),
        "it lives in the NAMESPACE meta dir, which purviews introduce; got {}",
        path.display(),
    );
    // Read back THROUGH the tool rather than by matching the file text: NUON
    // renders a string bare or quoted by content, so raw text is never the
    // thing to assert on.
    assert_eq!(
        patterns_of(&s, "iter/one"),
        Some(vec!["sourcetrait/pvtwo:".to_string()]),
    );
}

#[tested]
fn configure_returns_everything_the_purview_reveals() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_rig(&s, &t.temp_dir().join("pvshow"), "sourcetrait/pvshow");

    // NOT in view - the session is on `default` - and it still reports, because
    // the point is to CHECK the configuration you just wrote.
    let env = s.purview_configure("shown", &["sourcetrait/pvshow:"]);
    let block = env["signatures"]
        .as_str()
        .unwrap_or_else(|| panic!("configure reports the whole set: {env}"));
    assert!(block.contains("pvshow"), "got {env}");
    assert!(
        env.get("added").is_none() && env.get("removed").is_none(),
        "a FULL set, never a delta - a delta answers nothing about a purview \
         the session is not looking through; got {env}",
    );
}

#[tested]
fn an_empty_pattern_list_deletes_the_purview() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_rig(&s, &t.temp_dir().join("pvthree"), "sourcetrait/pvthree");

    let _ = s.purview_configure("gone/soon", &["sourcetrait/pvthree:"]);
    let after = s.purview_configure("gone/soon", &[]);
    assert!(!has_error(&after), "delete failed: {after}");
    assert_eq!(
        after["signatures"].as_str(),
        None,
        "a deleted purview reveals nothing, so `signatures` is null; got {after}",
    );
    assert!(patterns_of(&s, "gone/soon").is_none(), "and the row is gone");
}

#[test]
fn derived_and_malformed_ids_are_refused() {
    let s = TestServer::new();
    for id in [".", "*"] {
        let env = s.purview_configure(id, &["*"]);
        assert_eq!(
            error_kind(&env),
            Some("purview::invalid_id"),
            "`{id}` is DERIVED - computed, never stored, so configuring it is \
             meaningless rather than merely disallowed; got {env}",
        );
    }
    for id in ["/leading", "./relative", "trailing/", "double//slash", "Caps", "9lead", ""] {
        let env = s.purview_configure(id, &["*"]);
        assert_eq!(error_kind(&env), Some("purview::invalid_id"), "`{id}`: {env}");
    }
}

#[tested]
fn the_current_view_filters_what_info_reports() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_rig(&s, &t.temp_dir().join("pvseen"), "sourcetrait/pvseen");
    commit_rig(&s, &t.temp_dir().join("pvhidden"), "sourcetrait/pvhidden");

    let _ = s.purview_configure("default", &["sourcetrait/pvseen:"]);
    let info = s.info();
    let block = signatures(&info);
    assert!(block.contains("pvseen"), "got {info}");
    assert!(
        !block.contains("pvhidden"),
        "a rig outside the purview is NOT in view - the whole point of the \
         feature, and what keeps info() small; got {info}",
    );
}

#[tested]
fn blinders_render_another_purview_without_changing_the_current_one() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_rig(&s, &t.temp_dir().join("pvmine"), "sourcetrait/pvmine");
    commit_rig(&s, &t.temp_dir().join("pvtheirs"), "sourcetrait/pvtheirs");

    let _ = s.purview_configure("default", &["sourcetrait/pvmine:"]);
    let _ = s.purview_configure("sub/agent", &["sourcetrait/pvtheirs:"]);

    let blinded = s.info_as(&["sub/agent"]);
    let block = signatures(&blinded);
    assert!(block.contains("pvtheirs") && !block.contains("pvmine"), "got {blinded}");

    // Blinders are a RENDERING, not a mutation - which is what lets one session
    // hand a subagent a narrower surface without giving up its own.
    let mine = s.info();
    assert!(signatures(&mine).contains("pvmine"), "got {mine}");
    assert_eq!(mine["purview"][0][0].as_str(), Some("default"));
}

#[tested]
fn an_alias_names_the_same_purview_as_a_bare_id() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_rig(&s, &t.temp_dir().join("pvalias"), "sourcetrait/pvalias");
    let _ = s.purview_configure("aliased", &["sourcetrait/pvalias:"]);

    let bare = s.info_as(&["aliased"]);
    let at = s.info_as(&["@aliased"]);
    assert_eq!(
        signatures(&bare),
        signatures(&at),
        "`@aliased` and `aliased` name the same purview, so a caller can paste \
         a reference straight out of a configuration",
    );
}

#[tested]
fn setting_the_view_replaces_it_and_an_empty_list_means_default() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_rig(&s, &t.temp_dir().join("pvset"), "sourcetrait/pvset");
    let _ = s.purview_configure("only/set", &["sourcetrait/pvset:"]);

    let env = s.purview(&["only/set"]);
    assert!(!has_error(&env), "purview failed: {env}");
    let info = s.info();
    assert_eq!(
        info["purview"][0][0].as_str(),
        Some("only/set"),
        "setting REPLACES the view rather than adding to it; got {info}",
    );

    let back = s.purview(&[]);
    assert!(!has_error(&back), "got {back}");
    let info = s.info();
    assert_eq!(
        info["purview"][0][0].as_str(),
        Some("default"),
        "an EMPTY list means `default` - which is what makes a separate reset \
         tool unnecessary; got {info}",
    );
}

#[tested]
fn extending_is_additive_and_reveals_only_what_was_not_visible() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_rig(&s, &t.temp_dir().join("pvbase"), "sourcetrait/pvbase");
    commit_rig(&s, &t.temp_dir().join("pvextra"), "sourcetrait/pvextra");
    let _ = s.purview_configure("base", &["sourcetrait/pvbase:"]);
    let _ = s.purview_configure("extra", &["sourcetrait/pvextra:"]);
    // A DIFFERENT id naming a rig `base` already covers.
    let _ = s.purview_configure("dup", &["sourcetrait/pvbase:"]);
    let _ = s.purview(&["base"]);

    let env = s.purview_extend(&["extra"]);
    let revealed = env["revealed"].as_str().unwrap_or_default();
    assert!(
        revealed.contains("pvextra") && !revealed.contains("pvbase"),
        "`revealed` is what came INTO view, not the whole new view; got {env}",
    );
    assert_eq!(
        env["current"].as_array().map(|a| a.len()),
        Some(2),
        "additive: nothing already in view is disturbed; got {env}",
    );

    let dup = s.purview_extend(&["dup"]);
    assert_eq!(
        dup["revealed"].as_str(),
        None,
        "`dup` names a rig ALREADY in view under another purview id, so it \
         reveals nothing - the delta is over what is VISIBLE, not over the \
         pattern strings; got {dup}",
    );
}

#[test]
fn naming_an_unknown_purview_is_refused() {
    let s = TestServer::new();
    for env in [s.purview_extend(&["never/configured"]), s.purview(&["never/configured"])] {
        assert_eq!(
            error_kind(&env),
            Some("purview::invalid_id"),
            "an unknown id must be REFUSED rather than silently contributing \
             nothing - a typo would otherwise look exactly like an empty \
             purview; got {env}",
        );
    }
}

#[tested]
fn a_purview_may_hold_an_exact_call_as_well_as_patterns() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_rig(&s, &t.temp_dir().join("pvexact"), "sourcetrait/pvexact");

    let _ = s.purview_configure("default", &["sourcetrait/pvexact:m:go"]);
    let info = s.info();
    assert_eq!(
        rig_block(signatures(&info), "pvexact"),
        " pvexact\n  m\n   go <x:int> <out:int>\n",
        "an EXACT namepath is a legal purview value, and it brings its \
         ancestors with it so the block still spells a namepath; got {info}",
    );
}

#[tested]
fn the_dot_pattern_inspects_the_current_purview() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_rig(&s, &t.temp_dir().join("pvdot"), "sourcetrait/pvdot");
    commit_rig(&s, &t.temp_dir().join("pvnotdot"), "sourcetrait/pvnotdot");
    let _ = s.purview_configure("default", &["sourcetrait/pvdot:"]);

    let dot = s.inspect(".");
    let block = dot["doc"]["signatures"]
        .as_str()
        .unwrap_or_else(|| panic!("`.` should render a signatures block: {dot}"));
    assert!(
        block.contains("pvdot") && !block.contains("pvnotdot"),
        "`.` is the CURRENT PURVIEW. The parser classifies it and deliberately \
         leaves it unresolved, so purview is the only thing that can fill it in \
         - an empty block here means that stub was never filled; got {dot}",
    );
    let info = s.info();
    assert_eq!(block, signatures(&info), "and `.` is the SAME view info() reports");
}

#[tested]
fn a_reference_expands_only_when_filtering_and_a_cycle_flattens() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_rig(&s, &t.temp_dir().join("pvref"), "sourcetrait/pvref");

    let _ = s.purview_configure("base", &["sourcetrait/pvref:"]);
    // `@chain` pointing at its own purview is LEGAL - the visited set makes the
    // revisit contribute nothing rather than lock up.
    let env = s.purview_configure("chain", &["@base", "@chain"]);
    assert!(!has_error(&env), "a cycle is legal to write: {env}");
    assert_eq!(
        patterns_of(&s, "chain"),
        Some(vec!["@base".to_string(), "@chain".to_string()]),
        "reports stay RAW - a reference is stored and shown verbatim, never \
         expanded for display",
    );

    let _ = s.purview_configure("default", &["@chain"]);
    let info = s.info();
    assert!(
        signatures(&info).contains("pvref"),
        "but FILTERING expands, so `default -> @chain -> @base -> \
         sourcetrait/pvref:` puts the rig in view - and the self-cycle \
         terminated instead of hanging to get here; got {info}",
    );
}

#[tested]
fn a_dangling_reference_is_pruned_like_any_other_pattern() {
    let _t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);

    let env = s.purview_configure("orphan", &["@nosuch", "*"]);
    assert!(!has_error(&env), "configure failed: {env}");
    assert_eq!(
        patterns_of(&s, "orphan"),
        Some(vec!["*".to_string()]),
        "a reference to a purview with no row is a dangling namepath pattern, \
         and the design prunes those on detection; `*` needs nothing and stays",
    );
}

#[test]
fn a_reference_to_a_derived_purview_is_refused() {
    let s = TestServer::new();
    for value in ["@.", "@*"] {
        let env = s.purview_configure("derived/ref", &[value]);
        assert_eq!(
            error_kind(&env),
            Some("purview::invalid_id"),
            "`{value}` references a DERIVED built-in, which is never a row - so \
             it can reference nothing; got {env}",
        );
    }
}

#[tested]
fn a_dangling_pattern_is_pruned_when_the_table_is_next_written() {
    let _t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);

    let env = s.purview_configure("ghosts", &["humbletodd/", "*"]);
    assert!(!has_error(&env), "configure failed: {env}");
    assert_eq!(
        patterns_of(&s, "ghosts"),
        Some(vec!["*".to_string()]),
        "an author with no installed rig DANGLES and is dropped on detection",
    );
}

#[tested]
fn a_purview_pruned_down_to_nothing_is_dropped_rather_than_left_empty() {
    let _t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);

    let env = s.purview_configure("all/ghosts", &["humbletodd/"]);
    assert!(!has_error(&env), "configure failed: {env}");
    assert!(
        patterns_of(&s, "all/ghosts").is_none(),
        "every pattern dangled, so nothing is left to configure - and an empty \
         list is ALREADY the delete operation, so keeping the row would be a \
         state the tool surface cannot otherwise produce; got {env}",
    );
}

#[tested]
fn installing_adds_the_rig_to_default_without_narrowing_the_view() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_rig(&s, &t.temp_dir().join("pvprior"), "sourcetrait/pvprior");

    let src = t.temp_dir().join("pvfresh");
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export module go\nexport use go\n");
    write_source(
        &src,
        "m/go/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    let installed = s.rig("install", "sourcetrait/pvfresh", src.to_str().unwrap());
    assert!(!has_error(&installed), "install failed: {installed}");

    let patterns = patterns_of(&s, "default").expect("default always has a row");
    assert!(
        patterns.contains(&"*".to_string())
            && patterns.contains(&"sourcetrait/pvfresh:".to_string()),
        "install APPENDS beside `default`'s `['*']` rather than replacing it - \
         otherwise installing one rig would narrow everything down to it; got \
         {patterns:?}",
    );
    let info = s.info();
    let block = signatures(&info);
    assert!(block.contains("pvfresh") && block.contains("pvprior"), "nothing left view");
}

#[tested]
fn uninstalling_drops_its_pattern_from_every_purview() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    let src = t.temp_dir().join("pvtemp");
    commit_rig(&s, &src, "sourcetrait/pvtemp");
    let _ = s.purview_configure("holds/it", &["sourcetrait/pvtemp:", "*"]);

    let removed = s.rig("uninstall", "sourcetrait/pvtemp", src.to_str().unwrap());
    assert!(!has_error(&removed), "uninstall failed: {removed}");
    assert_eq!(
        patterns_of(&s, "holds/it"),
        Some(vec!["*".to_string()]),
        "the uninstalled rig's pattern is gone and `*` - which needs no \
         particular rig - survives",
    );
}
