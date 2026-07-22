use serde_json::json;
use sourcetrait_grammar_tests::*;
use sourcetrait_testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

#[tested]
fn default_id_is_user_env() {
    let t = testing::test!({ .using_temp_dir() });
    let src = t.temp_dir().join("src").join("mylib");
    let mut host = Host::spawn_full(t.temp_dir(), &[], &[("USER", "udefault")]);
    let resp = host.rig_new("sourcetrait/mylib", &src);
    assert!(!has_error_path(&resp), "rig(new) should succeed; got {resp}");
    let lib_dir = namespace_dir(&t.temp_dir().join("data"), "udefault", "default")
        .join("rigs")
        .join("rig")
        .join("sourcetrait")
        .join("mylib");
    assert!(
        lib_dir.exists(),
        "default-id namespace should land under <user>/default/; expected {}",
        lib_dir.display(),
    );
}

#[tested]
fn explicit_id_and_namespace_select_their_own_dirs() {
    let t = testing::test!({ .using_temp_dir() });
    let src = t.temp_dir().join("src").join("mylib");
    let mut host = Host::spawn_args(t.temp_dir(), &["--id", "aid", "--namespace", "ns1"]);
    let resp = host.rig_new("sourcetrait/mylib", &src);
    assert!(!has_error_path(&resp), "rig(new) should succeed; got {resp}");
    let lib_dir = namespace_dir(&t.temp_dir().join("data"), "aid", "ns1")
        .join("rigs")
        .join("rig")
        .join("sourcetrait")
        .join("mylib");
    assert!(lib_dir.exists(), "explicit --id/--namespace should select the namespace; expected {}", lib_dir.display());

    let info = host.call("info", json!({}));
    let env = structured(&info);
    assert_eq!(env["id"].as_str(), Some("aid"), "got {env}");
    assert_eq!(env["namespace"].as_str(), Some("ns1"), "got {env}");
}

#[tested]
fn namespaces_are_disjoint() {
    let t = testing::test!({ .using_temp_dir() });
    let src = t.temp_dir().join("src").join("nslib");

    {
        let mut ns1 = Host::spawn_args(t.temp_dir(), &["--id", "aid", "--namespace", "ns1"]);
        let _ = ns1.rig_new("sourcetrait/nslib", &src);
        write_source(&src, "mod.nu", "export module m\n");
        write_source(&src, "m/mod.nu", "export use double\n");
        write_source(
            &src,
            "m/double/mod.nu",
            &valid_function_source("x: int", "out: int", "{ out: ($args.x * 2) }"),
        );
        let committed = ns1.commit("sourcetrait/nslib");
        assert!(!has_error_path(&committed), "ns1 commit failed: {committed}");
        let called = ns1.call_np("sourcetrait/nslib:m:double", json!({"x": 4}));
        assert_eq!(structured(&called)["result"]["out"].as_i64(), Some(8), "ns1 call: {called}");
    }

    let mut ns2 = Host::spawn_args(t.temp_dir(), &["--id", "aid", "--namespace", "ns2"]);
    let info = ns2.call("info", json!({}));
    let block = structured(&info)["signatures"]
        .as_str()
        .expect("info carries a signatures block");
    assert!(
        block.is_empty(),
        "ns2 must not see ns1's rigs, so its block is empty rather than \
         absent; got {block:?}",
    );
    let called = ns2.call_np("sourcetrait/nslib:m:double", json!({"x": 4}));
    assert!(has_error_path(&called), "ns2 call into ns1's rig must fail; got {called}");

    assert!(namespace_dir(&t.temp_dir().join("data"), "aid", "ns1").exists());
    assert!(namespace_dir(&t.temp_dir().join("data"), "aid", "ns2").exists());
}

#[tested]
fn env_carries_id_namespace_and_work_dir() {
    let t = testing::test!({ .using_temp_dir() });
    let wd = t.temp_dir().join("wd");
    std::fs::create_dir_all(&wd).unwrap();
    let wd_str = wd.to_str().unwrap();
    let mut host = Host::spawn_args(
        t.temp_dir(),
        &["--id", "envid", "--namespace", "envns", "--workdir", wd_str],
    );
    let resp = host.run(json!({
        "args_schema": {},
        "result_schema": {"id": "string", "ns": "string", "wd": "string"},
        "args": {},
        "body": "{ id: $env.EQUIP_ID, ns: $env.EQUIP_NAMESPACE, wd: $env.EQUIP_WORK_DIR }",
    }));
    let env = structured(&resp);
    assert_eq!(env["result"]["id"].as_str(), Some("envid"), "EQUIP_ID; got {resp}");
    assert_eq!(env["result"]["ns"].as_str(), Some("envns"), "EQUIP_NAMESPACE; got {resp}");
    assert_eq!(env["result"]["wd"].as_str(), Some(wd_str), "EQUIP_WORK_DIR; got {resp}");
    let info = host.call("info", json!({}));
    assert_eq!(structured(&info)["work_dir"].as_str(), Some(wd_str), "info work_dir; got {info}");
}

#[tested]
fn workdir_tilde_expands_against_home() {
    let t = testing::test!({ .using_temp_dir() });
    let home = t.temp_dir().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let home_str = home.to_str().unwrap();
    let mut host = Host::spawn_full(
        t.temp_dir(),
        &["--id", "tildeid", "--workdir", "~/wd_x"],
        &[("HOME", home_str)],
    );
    let resp = host.run(json!({
        "args_schema": {},
        "result_schema": {"wd": "string"},
        "args": {},
        "body": "{ wd: $env.EQUIP_WORK_DIR }",
    }));
    let expected = home.join("wd_x");
    assert_eq!(
        structured(&resp)["result"]["wd"].as_str(),
        expected.to_str(),
        "a ~/ workdir should expand against HOME; got {resp}",
    );
}

#[tested]
fn workdir_defaults_under_home_proj_equip_id() {
    let t = testing::test!({ .using_temp_dir() });
    let home = t.temp_dir().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let home_str = home.to_str().unwrap();
    let mut host = Host::spawn_full(t.temp_dir(), &["--id", "wdid"], &[("HOME", home_str)]);
    let resp = host.run(json!({
        "args_schema": {},
        "result_schema": {"wd": "string"},
        "args": {},
        "body": "{ wd: $env.EQUIP_WORK_DIR }",
    }));
    let expected = home.join("proj").join("equip").join("wdid");
    assert_eq!(
        structured(&resp)["result"]["wd"].as_str(),
        expected.to_str(),
        "an absent --workdir should default to <home>/proj/equip/<id>; got {resp}",
    );
}
