use std::{path::PathBuf};
use sourcetrait_equipment as equipment;
use sourcetrait_testing::prelude::*;
use pretty_assertions::{assert_eq};

static TESTING: testing::Module = testing::module!(Integration, {
    .using_fixture_dir()
});

#[tested]
fn test_read() {
    let test = testing::test!({
        .inherit_fixture_dir()
    });

    let expected: equipment::Rig = equipment::Rig {
        name: equipment::Key::from("empower"),
        title: equipment::Title::from("Empower"),
        version: equipment::Version::from("0.0.0-2"),
        provider: equipment::Provider::GitHub(equipment::GitHubProvider),
        author: equipment::Author {
            name: equipment::Key::from("sourcetrait"),
            title: equipment::Title::from("SourceTrait"),
            email: equipment::Email::from("development@sourcetrait.com"),
        },
        description: equipment::Description {
            summary: equipment::Summary::from("Empower rig"),
            keywords: Vec::from([
                equipment::Keyword::from("ai"),
            ]),
        },
        license: equipment::License {
            spdx: Some(equipment::Spdx::from("AGPL-3.0-or-later")),
            file: PathBuf::from("LICENSE-AGPL-3.txt"),
        },
        exports: equipment::RigExports {
            libraries: Vec::from([
                equipment::LibraryName::from("ant"),
                equipment::LibraryName::from("drone"),
                equipment::LibraryName::from("equip"),
                equipment::LibraryName::from("empower"),
                equipment::LibraryName::from("fae"),
                equipment::LibraryName::from("queen"),
            ]),
        },
        nushell: equipment::RigNushell {
            version: equipment::Version::from("0.113.1"),
        }, 
    };
    
    let actual = equipment::Rig::read(
            test.fixture_dir().join(equipment::RigToml::RIG_TOML)
        ).expect("reads");

    assert_eq!(expected, actual);
}