use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serde_json::json;
use sourcetrait_grammar_mcp::guts::{
    TestServer, error_kind, error_kinds, has_error, has_kind, valid_function_source, write_source,
};
use sourcetrait_common::testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

fn chmod_x(path: &Path) {
    let mut perms = std::fs::metadata(path).expect("metadata").permissions();
    perms.set_mode(perms.mode() | 0o755);
    std::fs::set_permissions(path, perms).expect("set +x");
}

fn author_valid_base(src: &Path) {
    write_source(src, "mod.nu", "export module m\n");
    write_source(src, "m/mod.nu", "export use double\n");
    write_source(
        src,
        "m/double/mod.nu",
        &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
    );
}

/// git-tracked paths under `name` in the in-process namespace's rigs repo (a
/// read-only inspection of the commit's output; git is a substrate tool).
fn git_ls_files(repo: &Path, name: &str) -> Vec<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("ls-files")
        .arg("--")
        .arg(name)
        .output()
        .expect("git ls-files");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect()
}

#[tested]
fn assets_data_files_carried_recursively() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("assetlib");
    let _ = s.rig("new", "sourcetrait/assetlib", src.to_str().unwrap());
    author_valid_base(&src);
    write_source(&src, ".assets/data.json", "{\"k\": 1}\n");
    write_source(&src, ".assets/notes.txt", "hello\n");
    write_source(&src, ".assets/locale/en/main.ftl", "greeting = hi\n");
    let env = s.commit("sourcetrait/assetlib");
    assert!(!has_error(&env), "assets commit should succeed; got {env}");
    let canon = s.rig_dir("sourcetrait/assetlib");
    assert!(canon.join(".assets/data.json").exists(), "data.json carried");
    assert!(canon.join(".assets/notes.txt").exists(), "notes.txt carried");
    assert!(
        canon.join(".assets/locale/en/main.ftl").exists(),
        "nested locale carried (recursive)"
    );
}

#[tested]
fn assets_denies_script_extension() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("assetshlib");
    let _ = s.rig("new", "sourcetrait/assetshlib", src.to_str().unwrap());
    author_valid_base(&src);
    write_source(&src, ".assets/run.sh", "echo hi\n");
    let env = s.commit("sourcetrait/assetshlib");
    assert!(
        has_kind(&env, "rig::asset_extension_denied"),
        "a .sh in .assets/ should be denied; got {:?}",
        error_kinds(&env)
    );
}

#[tested]
fn assets_dotfiles_judged_by_extension() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("assetdotlib");
    let _ = s.rig("new", "sourcetrait/assetdotlib", src.to_str().unwrap());
    author_valid_base(&src);
    write_source(&src, ".assets/.gitignore", "*.tmp\n");
    write_source(&src, ".assets/.foo", "opaque\n");
    let ok = s.commit("sourcetrait/assetdotlib");
    assert!(!has_error(&ok), "extension-less dotfiles in .assets/ pass; got {ok}");
    let canon = s.rig_dir("sourcetrait/assetdotlib");
    assert!(canon.join(".assets/.gitignore").exists());
    assert!(canon.join(".assets/.foo").exists());

    write_source(&src, ".assets/.foo.sh", "#!/bin/sh\n");
    let bad = s.commit("sourcetrait/assetdotlib");
    assert!(
        has_kind(&bad, "rig::asset_extension_denied"),
        ".foo.sh (ext sh) should be denied; got {:?}",
        error_kinds(&bad)
    );
}

#[tested]
fn assets_denies_executable_bit() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("assetxlib");
    let _ = s.rig("new", "sourcetrait/assetxlib", src.to_str().unwrap());
    author_valid_base(&src);
    write_source(&src, ".assets/data.json", "{}\n");
    chmod_x(&src.join(".assets/data.json"));
    let env = s.commit("sourcetrait/assetxlib");
    assert!(
        has_kind(&env, "rig::asset_executable_denied"),
        "a +x .assets/ file should be denied; got {:?}",
        error_kinds(&env)
    );
}

#[tested]
fn docs_md_txt_carried() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("doclib");
    let _ = s.rig("new", "sourcetrait/doclib", src.to_str().unwrap());
    author_valid_base(&src);
    write_source(&src, ".docs/guide.md", "# guide\n");
    write_source(&src, ".docs/notes.txt", "notes\n");
    let env = s.commit("sourcetrait/doclib");
    assert!(!has_error(&env), "docs commit should succeed; got {env}");
    let canon = s.rig_dir("sourcetrait/doclib");
    assert!(canon.join(".docs/guide.md").exists());
    assert!(canon.join(".docs/notes.txt").exists());
}

#[tested]
fn docs_denies_other_extensions_and_plain_dotfile() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("docdenylib");
    let _ = s.rig("new", "sourcetrait/docdenylib", src.to_str().unwrap());
    author_valid_base(&src);
    write_source(&src, ".docs/data.json", "{}\n");
    let json_env = s.commit("sourcetrait/docdenylib");
    assert!(
        has_kind(&json_env, "rig::doc_extension_denied"),
        ".docs/data.json should be denied; got {:?}",
        error_kinds(&json_env)
    );

    std::fs::remove_file(src.join(".docs/data.json")).unwrap();
    write_source(&src, ".docs/.foo", "x\n");
    let foo = s.commit("sourcetrait/docdenylib");
    assert!(
        has_kind(&foo, "rig::doc_extension_denied"),
        ".docs/.foo should be denied; got {:?}",
        error_kinds(&foo)
    );
}

#[tested]
fn docs_allows_gitignore() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("docgitlib");
    let _ = s.rig("new", "sourcetrait/docgitlib", src.to_str().unwrap());
    author_valid_base(&src);
    write_source(&src, ".docs/guide.md", "# guide\n");
    write_source(&src, ".docs/.gitignore", "*.bak\n");
    let env = s.commit("sourcetrait/docgitlib");
    assert!(!has_error(&env), ".docs/.gitignore is allowlisted; got {env}");
}

#[tested]
fn docs_denies_executable_bit() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("docxlib");
    let _ = s.rig("new", "sourcetrait/docxlib", src.to_str().unwrap());
    author_valid_base(&src);
    write_source(&src, ".docs/guide.md", "# guide\n");
    chmod_x(&src.join(".docs/guide.md"));
    let env = s.commit("sourcetrait/docxlib");
    assert!(
        has_kind(&env, "rig::doc_executable_denied"),
        "a +x .docs/ file should be denied; got {:?}",
        error_kinds(&env)
    );
}

#[tested]
fn root_sanctioned_files_carried() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("rootlib");
    let _ = s.rig("new", "sourcetrait/rootlib", src.to_str().unwrap());
    author_valid_base(&src);
    write_source(&src, "README.md", "# rootlib\n");
    write_source(&src, "LEGAL.md", "legal\n");
    write_source(&src, "LICENSE.txt", "license\n");
    write_source(&src, "LICENSE-MIT.txt", "mit\n");
    write_source(&src, "rig.toml", "name = \"rootlib\"\n");
    let env = s.commit("sourcetrait/rootlib");
    assert!(!has_error(&env), "root files should commit; got {env}");
    let canon = s.rig_dir("sourcetrait/rootlib");
    for f in ["README.md", "LEGAL.md", "LICENSE.txt", "LICENSE-MIT.txt", "rig.toml"] {
        assert!(canon.join(f).exists(), "{f} should be carried");
    }
}

#[tested]
fn root_denies_unexpected_non_nu_file() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("rootdenylib");
    let _ = s.rig("new", "sourcetrait/rootdenylib", src.to_str().unwrap());
    author_valid_base(&src);
    write_source(&src, "data.json", "{}\n");
    let env = s.commit("sourcetrait/rootdenylib");
    assert!(
        has_kind(&env, "rig::source_extension_denied"),
        "a stray root data.json should be denied; got {:?}",
        error_kinds(&env)
    );
}

#[tested]
fn sanctioned_file_in_module_dir_denied_root_only() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("modreadmelib");
    let _ = s.rig("new", "sourcetrait/modreadmelib", src.to_str().unwrap());
    author_valid_base(&src);
    write_source(&src, "m/README.md", "# nope\n");
    let env = s.commit("sourcetrait/modreadmelib");
    assert!(
        has_kind(&env, "rig::source_extension_denied"),
        "a module-level README.md should be denied (root-only); got {:?}",
        error_kinds(&env)
    );
}

#[tested]
fn executable_nu_file_denied() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("xnulib");
    let _ = s.rig("new", "sourcetrait/xnulib", src.to_str().unwrap());
    author_valid_base(&src);
    chmod_x(&src.join("m/double/mod.nu"));
    let env = s.commit("sourcetrait/xnulib");
    assert!(
        has_kind(&env, "rig::source_executable_denied"),
        "a +x .nu file should be denied; got {:?}",
        error_kinds(&env)
    );
}

#[tested]
fn gitignore_allowed_at_root_and_module() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("gitlib");
    let _ = s.rig("new", "sourcetrait/gitlib", src.to_str().unwrap());
    author_valid_base(&src);
    write_source(&src, ".gitignore", "*.tmp\n");
    write_source(&src, "m/.gitignore", "scratch/\n");
    let env = s.commit("sourcetrait/gitlib");
    assert!(!has_error(&env), ".gitignore is allowed at any depth; got {env}");
    let canon = s.rig_dir("sourcetrait/gitlib");
    assert!(canon.join(".gitignore").exists(), "root .gitignore carried");
    assert!(canon.join("m/.gitignore").exists(), "module .gitignore carried");
}

#[tested]
fn gitignore_honored_at_commit_staging_only() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("honorlib");
    let _ = s.rig("new", "sourcetrait/honorlib", src.to_str().unwrap());
    author_valid_base(&src);
    write_source(&src, ".gitignore", "ignored.md\n");
    write_source(&src, ".docs/ignored.md", "secret\n");
    write_source(&src, ".docs/kept.md", "public\n");
    let env = s.commit("sourcetrait/honorlib");
    assert!(!has_error(&env), "commit should succeed; got {env}");

    let canon = s.rig_dir("sourcetrait/honorlib");
    assert!(
        canon.join(".docs/ignored.md").exists(),
        "ignored file is still copied to the canonical on disk"
    );
    let tracked = git_ls_files(&s.rigs_dir(), "rig/sourcetrait/honorlib");
    assert!(
        tracked.iter().any(|p| p.ends_with(".docs/kept.md")),
        "kept.md should be tracked; got {tracked:?}"
    );
    assert!(
        tracked.iter().any(|p| p.ends_with("honorlib/.gitignore")),
        ".gitignore should be tracked; got {tracked:?}"
    );
    assert!(
        !tracked.iter().any(|p| p.ends_with(".docs/ignored.md")),
        "the gitignored file must NOT be tracked (honored at commit); got {tracked:?}"
    );
}

#[tested]
fn nested_assets_dir_is_skipped_not_carried() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("nestlib");
    let _ = s.rig("new", "sourcetrait/nestlib", src.to_str().unwrap());
    author_valid_base(&src);
    write_source(&src, "m/.assets/data.json", "{}\n");
    let env = s.commit("sourcetrait/nestlib");
    assert!(
        !has_error(&env),
        "a nested .assets is just a skipped dotfile; commit should succeed; got {env}"
    );
    assert!(
        !s.rig_dir("sourcetrait/nestlib").join("m/.assets").exists(),
        "a nested .assets must not be carried"
    );
}

#[tested]
fn rig_name_denylist() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    for denied in ["docs", "tools", "bin", "target"] {
        let src = t.temp_dir().join(format!("src_{denied}"));
        let env = s.rig("new", &format!("sourcetrait/{denied}"), src.to_str().unwrap());
        assert_eq!(
            error_kind(&env),
            Some("rig::name_denied"),
            "rig name `{denied}` should be denied; got {env}"
        );
    }
    let src = t.temp_dir().join("okname");
    let ok = s.rig("new", "sourcetrait/okname", src.to_str().unwrap());
    assert!(!has_error(&ok), "a normal name should succeed; got {ok}");
}

#[tested]
fn install_carries_assets_and_is_callable() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let src = t.temp_dir().join("shipassetlib");
    author_valid_base(&src);
    write_source(&src, ".assets/locale/en/main.ftl", "k = v\n");
    write_source(&src, "README.md", "# ship\n");
    let env = s.rig("install", "sourcetrait/shipassetlib", src.to_str().unwrap());
    assert!(!has_error(&env), "install should succeed; got {env}");
    let canon = s.rig_dir("sourcetrait/shipassetlib");
    assert!(
        canon.join(".assets/locale/en/main.ftl").exists(),
        "install must carry .assets/ too"
    );
    assert!(canon.join("README.md").exists(), "install carries root README.md");
    let called = s.call("sourcetrait/shipassetlib:m:double", json!({"x": 21}));
    assert_eq!(
        called["result"]["out"].as_i64(),
        Some(42),
        "installed rig should be callable; got {called}"
    );
}
