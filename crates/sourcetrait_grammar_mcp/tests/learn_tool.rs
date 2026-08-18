use sourcetrait_grammar_mcp::guts::TestServer;
use sourcetrait_common::testing::prelude::*;

static TESTING: testing::Module = testing::module!(Integration, { .using_temp_dir() });

#[tested]
fn learn_writes_versioned_skill() {
    let t = testing::test!({ .using_temp_dir() });
    let s = TestServer::new();
    let harness = t.temp_dir().join("harness");

    let env = s.learn(harness.to_str().unwrap());
    assert!(env.get("error").is_none(), "learn returned an error: {env}");

    let version = env["version"].as_str().expect("version field");
    assert_eq!(
        version,
        env!("CARGO_PKG_VERSION"),
        "stamped version should match the crate version",
    );

    let bytes = env["bytes"].as_u64().expect("bytes field");
    assert!(bytes > 0, "written skill should be non-empty");

    let written = env["written_path"].as_str().expect("written_path field");
    let expected = harness.join("skills").join("nu").join("SKILL.md");
    assert_eq!(
        written,
        expected.to_str().unwrap(),
        "written_path should be <harness>/skills/nu/SKILL.md",
    );

    let body = std::fs::read_to_string(&expected).expect("read written skill");
    assert_eq!(
        body.len() as u64,
        bytes,
        "reported bytes should match the written file length",
    );
    assert!(
        !body.contains("{{ version }}"),
        "template should be rendered, not raw liquid",
    );
    assert!(
        body.contains(&format!("server v{version}")),
        "stamp line should carry the live version v{version}",
    );
    assert!(
        body.contains("name: nu"),
        "generated skill should retain its frontmatter",
    );
}
