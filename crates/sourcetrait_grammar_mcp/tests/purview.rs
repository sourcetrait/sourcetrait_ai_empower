//! Purviews: the namespace-level scoped view of the callable surface.
//!
//! Its OWN test binary, because a purview table is per STORE and the in-process
//! store is shared per binary - these tests mutate it, so they must not share it
//! with tests that assume a whole-store view.
//!
//! Each test resets the table first and uses unique library and purview names,
//! so nothing here depends on the order cargo happens to run them in.

use std::path::Path;

use sourcetrait_grammar_mcp::guts::{
    TestServer, error_kind, has_error, library_block, valid_function_source, write_source,
};
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

/// Commit a one-call library, so a purview has something concrete to include or
/// exclude.
fn commit_lib(
    s: &TestServer,
    src: &Path,
    name: &str,
) {
    let _ = s.library("new", name, src.to_str().unwrap());
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

/// Back to an unconfigured namespace with the session at `default`.
fn reset_purviews(s: &TestServer) {
    let listed = s.purview_list();
    if let Some(rows) = listed["purviews"].as_array() {
        for row in rows.clone() {
            if let Some(id) = row[0].as_str() {
                let _ = s.purview_configure(id, &[]);
            }
        }
    }
    let _ = s.purview_reset();
}

fn signatures(env: &serde_json::Value) -> &str {
    env["signatures"].as_str().expect("signatures block")
}

#[test]
#[named]
fn an_unconfigured_namespace_sees_everything() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_lib(&s, &t.temp_dir().join("pvone"), "sourcetrait/pvone");

    let info = s.info();
    assert_eq!(
        info["purview"][0][0].as_str(),
        Some("default"),
        "a session starts at `default`; got {info}",
    );
    assert_eq!(
        info["purview"][0][1][0].as_str(),
        Some("*"),
        "an UNCONFIGURED default resolves to everything, which is what makes a \
         fresh namespace usable before anyone configures anything; got {info}",
    );
    assert!(
        signatures(&info).contains("pvone"),
        "and everything means the library is in view; got {info}",
    );
}

#[test]
#[named]
fn configuring_writes_the_namespace_meta_file_and_reads_back() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_lib(&s, &t.temp_dir().join("pvtwo"), "sourcetrait/pvtwo");

    let env = s.purview_configure("iter/one", &["sourcetrait/pvtwo:"]);
    assert!(!has_error(&env), "configure failed: {env}");
    let path = s.purviews_path();
    assert!(path.exists(), "the table should be written at {}", path.display());
    assert!(
        path.ends_with("purviews.nuon") && path.parent().is_some_and(|p| p.ends_with(".meta")),
        "it lives in the NAMESPACE meta dir, which purviews introduce; got {}",
        path.display(),
    );

    // Read back THROUGH the tool rather than by matching the file text: NUON
    // renders a string bare or quoted depending on its content, so raw text is
    // never the thing to assert on.
    let listed = s.purview_list();
    let row = listed["purviews"]
        .as_array()
        .expect("purviews array")
        .iter()
        .find(|r| r[0].as_str() == Some("iter/one"))
        .unwrap_or_else(|| panic!("iter/one absent: {listed}"));
    assert_eq!(row[1][0].as_str(), Some("sourcetrait/pvtwo:"));
}

#[test]
#[named]
fn an_empty_selector_list_deletes_the_purview() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_lib(&s, &t.temp_dir().join("pvthree"), "sourcetrait/pvthree");

    let _ = s.purview_configure("gone/soon", &["sourcetrait/pvthree:"]);
    let after = s.purview_configure("gone/soon", &[]);
    assert!(!has_error(&after), "delete failed: {after}");
    assert!(
        !after["purviews"]
            .as_array()
            .expect("purviews")
            .iter()
            .any(|r| r[0].as_str() == Some("gone/soon")),
        "an empty list is the delete operation; got {after}",
    );
}

#[test]
fn derived_and_malformed_ids_are_refused() {
    let s = TestServer::new();
    for id in [".", "*"] {
        let env = s.purview_configure(id, &["*"]);
        assert_eq!(
            error_kind(&env),
            Some("purview::invalid_id"),
            "`{id}` is DERIVED - it is computed, never stored, so configuring it \
             is meaningless rather than merely disallowed; got {env}",
        );
    }
    for id in ["/leading", "./relative", "trailing/", "double//slash", "Caps", "9lead", ""] {
        let env = s.purview_configure(id, &["*"]);
        assert_eq!(
            error_kind(&env),
            Some("purview::invalid_id"),
            "`{id}` is not a bare-relative snake label; got {env}",
        );
    }
}

#[test]
#[named]
fn the_current_view_filters_what_info_reports() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_lib(&s, &t.temp_dir().join("pvseen"), "sourcetrait/pvseen");
    commit_lib(&s, &t.temp_dir().join("pvhidden"), "sourcetrait/pvhidden");

    let _ = s.purview_configure("default", &["sourcetrait/pvseen:"]);
    let info = s.info();
    let block = signatures(&info);
    assert!(block.contains("pvseen"), "the configured library is in view; got {info}");
    assert!(
        !block.contains("pvhidden"),
        "and a library outside the purview is NOT - that is the whole point of \
         the feature, and it is what keeps info() small; got {info}",
    );
    assert_eq!(info["purview"][0][1][0].as_str(), Some("sourcetrait/pvseen:"));
}

#[test]
#[named]
fn extend_reports_what_arrived_and_reset_reports_what_left() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_lib(&s, &t.temp_dir().join("pvbase"), "sourcetrait/pvbase");
    commit_lib(&s, &t.temp_dir().join("pvextra"), "sourcetrait/pvextra");

    let _ = s.purview_configure("default", &["sourcetrait/pvbase:"]);
    let _ = s.purview_configure("side/car", &["sourcetrait/pvextra:"]);

    let extended = s.purview_extend(&["side/car"]);
    assert!(!has_error(&extended), "extend failed: {extended}");
    let added = extended["added"].as_str().unwrap_or_default();
    assert!(
        added.contains("pvextra") && !added.contains("pvbase"),
        "`added` is the block for what CAME INTO view, not the whole new view; \
         got {extended}",
    );
    assert!(
        extended["removed"].is_null(),
        "extending is additive, so nothing left; got {extended}",
    );
    assert!(signatures(&s.info()).contains("pvextra"), "and the view really widened");

    let reset = s.purview_reset();
    assert!(!has_error(&reset), "reset failed: {reset}");
    assert_eq!(
        reset["removed"].as_array().map(|a| a.len()),
        Some(1),
        "reset drops back to default, so the extra selector left; got {reset}",
    );
    assert_eq!(reset["removed"][0].as_str(), Some("sourcetrait/pvextra:"));
    assert!(
        !signatures(&s.info()).contains("pvextra"),
        "and the view really narrowed again",
    );
}

#[test]
#[named]
fn blinders_render_another_purview_without_changing_the_current_one() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_lib(&s, &t.temp_dir().join("pvmine"), "sourcetrait/pvmine");
    commit_lib(&s, &t.temp_dir().join("pvtheirs"), "sourcetrait/pvtheirs");

    let _ = s.purview_configure("default", &["sourcetrait/pvmine:"]);
    let _ = s.purview_configure("sub/agent", &["sourcetrait/pvtheirs:"]);

    let blinded = s.info_as(&["sub/agent"]);
    let block = signatures(&blinded);
    assert!(block.contains("pvtheirs") && !block.contains("pvmine"), "got {blinded}");
    assert_eq!(blinded["purview"][0][0].as_str(), Some("sub/agent"));

    // The session's own view is untouched: blinders are a RENDERING, not a
    // mutation, which is what lets one session hand a subagent a narrower view
    // without giving up its own.
    let mine = s.info();
    assert!(signatures(&mine).contains("pvmine"), "got {mine}");
    assert_eq!(mine["purview"][0][0].as_str(), Some("default"));
}

#[test]
#[named]
fn installing_adds_the_rig_to_default_without_narrowing_the_view() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_lib(&s, &t.temp_dir().join("pvprior"), "sourcetrait/pvprior");

    let src = t.temp_dir().join("pvfresh");
    write_source(&src, "mod.nu", "export module m\n");
    write_source(&src, "m/mod.nu", "export module go\nexport use go\n");
    write_source(
        &src,
        "m/go/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: $args.x }"),
    );
    let installed = s.library("install", "sourcetrait/pvfresh", src.to_str().unwrap());
    assert!(!has_error(&installed), "install failed: {installed}");

    let listed = s.purview_list();
    let default_row = listed["purviews"]
        .as_array()
        .expect("purviews")
        .iter()
        .find(|r| r[0].as_str() == Some("default"))
        .unwrap_or_else(|| panic!("install should have materialized default: {listed}"));
    let selectors: Vec<&str> = default_row[1]
        .as_array()
        .expect("selectors")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        selectors.contains(&"*"),
        "an UNCONFIGURED default is implicitly everything, so the install must \
         materialize that `*` FIRST - otherwise installing one library would \
         narrow the whole store down to it; got {selectors:?}",
    );
    assert!(selectors.contains(&"sourcetrait/pvfresh:"), "got {selectors:?}");

    let info = s.info();
    let block = signatures(&info);
    assert!(block.contains("pvfresh") && block.contains("pvprior"), "nothing left view");
}

#[test]
#[named]
fn uninstalling_drops_its_pattern_from_every_purview() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    let src = t.temp_dir().join("pvtemp");
    commit_lib(&s, &src, "sourcetrait/pvtemp");
    let _ = s.purview_configure("holds/it", &["sourcetrait/pvtemp:", "*"]);

    let removed = s.library("uninstall", "sourcetrait/pvtemp", src.to_str().unwrap());
    assert!(!has_error(&removed), "uninstall failed: {removed}");

    let listed = s.purview_list();
    let row = listed["purviews"]
        .as_array()
        .expect("purviews")
        .iter()
        .find(|r| r[0].as_str() == Some("holds/it"))
        .unwrap_or_else(|| panic!("holds/it absent: {listed}"));
    let selectors: Vec<&str> = row[1]
        .as_array()
        .expect("selectors")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(
        selectors,
        vec!["*"],
        "the uninstalled rig's pattern is gone and `*` - which needs no \
         particular library - survives; got {selectors:?}",
    );
}

#[test]
#[named]
fn a_dangling_selector_is_pruned_when_the_table_is_next_written() {
    let _t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);

    let env = s.purview_configure("ghosts", &["humbletodd/", "*"]);
    assert!(!has_error(&env), "configure failed: {env}");
    assert_eq!(
        env["pruned"].as_array().map(|a| a.len()),
        Some(1),
        "an author with no installed rig is DANGLING and is dropped on detection; \
         got {env}",
    );
    assert_eq!(env["pruned"][0].as_str(), Some("humbletodd/"));
    assert!(
        env["purviews"]
            .as_array()
            .expect("purviews")
            .iter()
            .any(|r| r[0].as_str() == Some("ghosts")),
        "`*` survived, so the purview still has something to show; got {env}",
    );
}

#[test]
#[named]
fn a_purview_pruned_down_to_nothing_is_dropped_rather_than_left_empty() {
    let _t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);

    let env = s.purview_configure("all/ghosts", &["humbletodd/"]);
    assert!(!has_error(&env), "configure failed: {env}");
    assert!(
        !env["purviews"]
            .as_array()
            .expect("purviews")
            .iter()
            .any(|r| r[0].as_str() == Some("all/ghosts")),
        "every selector dangled, so nothing is left to configure - and an empty \
         list is ALREADY the delete operation, so keeping the row would be a \
         state the tool surface cannot otherwise produce; got {env}",
    );
}

#[test]
fn extending_with_an_unknown_purview_is_refused() {
    let s = TestServer::new();
    let env = s.purview_extend(&["never/configured/this"]);
    assert_eq!(
        error_kind(&env),
        Some("purview::invalid_id"),
        "an unknown id must be REFUSED rather than silently contribute nothing - \
         a typo would otherwise look exactly like an empty purview; got {env}",
    );
}

#[test]
#[named]
fn a_purview_may_hold_an_exact_call_as_well_as_patterns() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    reset_purviews(&s);
    commit_lib(&s, &t.temp_dir().join("pvexact"), "sourcetrait/pvexact");

    let _ = s.purview_configure("default", &["sourcetrait/pvexact:m:go"]);
    let info = s.info();
    let block = signatures(&info);
    assert_eq!(
        library_block(block, "pvexact"),
        " pvexact\n  m\n   go <x:int> <out:int>\n",
        "an EXACT namepath is a legal purview value, and it brings its ancestors \
         with it so the block still spells a namepath; got {block:?}",
    );
}
