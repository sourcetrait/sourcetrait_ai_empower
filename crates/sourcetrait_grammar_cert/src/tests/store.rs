use crate::store::{KNOWN, TrustStore, TrustStoreKind, TrustTarget, resolve_target};

/// The table is a claim about real systems, so guard the details that are easy to get
/// wrong and impossible to notice.
#[test]
fn known_layouts_are_well_formed() {
    assert!(!KNOWN.is_empty());
    for store in KNOWN {
        match store {
            TrustStore::TrustDir(s) => {
                assert!(s.trust_dir.starts_with('/'), "{} trust dir must be absolute", s.id);
                assert!(s.bundle.starts_with('/'), "{} bundle must be absolute", s.id);
                assert!(!s.cert_extension.starts_with('.'), "{} joins the dot itself", s.id);
                assert!(!s.update_command.is_empty());
            }
            TrustStore::Keychain(s) => {
                assert!(s.program.starts_with('/'), "{} program must be absolute", s.id);
                assert!(s.keychain.starts_with('/'), "{} keychain must be absolute", s.id);
            }
        }
    }
}

#[test]
fn update_ca_certificates_requires_crt() {
    let debian = KNOWN
        .iter()
        .find_map(|s| match s {
            TrustStore::TrustDir(d) if d.update_command == "update-ca-certificates" => Some(d),
            _ => None,
        })
        .expect("the ca-certificates layout is in the table");
    assert_eq!(
        debian.cert_extension, "crt",
        "update-ca-certificates only processes .crt, so a .pem cert would be ignored",
    );
}

#[test]
fn ids_are_unique() {
    let mut ids: Vec<&str> = KNOWN.iter().map(TrustStore::id).collect();
    ids.sort_unstable();
    let before = ids.len();
    ids.dedup();
    assert_eq!(before, ids.len());
}

#[test]
fn kind_mirrors_the_variant() {
    for store in KNOWN {
        let expected = match store {
            TrustStore::TrustDir(_) => TrustStoreKind::TrustDir,
            TrustStore::Keychain(_) => TrustStoreKind::Keychain,
        };
        assert_eq!(store.kind(), expected);
    }
}

/// `install` must never demand a flag it does not expose. An earlier cut shared one
/// resolver with `verify`, so `--trust-dir` asked for a `--bundle` that install has no
/// way to supply - unreachable on any keychain platform.
#[test]
fn install_resolution_never_asks_for_a_bundle() {
    let (target, _) = resolve_target(
        Some(std::path::Path::new("/tmp/trust")),
        Some("true"),
    )
    .expect("a trust dir plus a refresh command is sufficient to install");
    match target {
        TrustTarget::Dir {
            dir,
            update_command,
            ..
        } => {
            assert_eq!(dir, std::path::Path::new("/tmp/trust"));
            assert_eq!(update_command, "true");
        }
        TrustTarget::Keychain { .. } => panic!("an explicit trust dir must select the dir model"),
    }
}

/// The file dropped into a SHARED system dir carries the vendor prefix; a bare
/// `<name>.pem` there collides and says nothing about who installed it.
#[test]
fn trust_file_name_is_vendor_prefixed() {
    assert_eq!(crate::install::trust_file_name("grammar", "pem"), "sourcetrait_grammar.pem");
    assert_eq!(crate::install::trust_file_name("test", "crt"), "sourcetrait_test.crt");
}
