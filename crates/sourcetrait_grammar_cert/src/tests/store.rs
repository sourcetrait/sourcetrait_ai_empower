use crate::store::{KNOWN, Placement, TrustStore, TrustStoreKind, resolve};

/// The table is a claim about real systems, so guard the details that are easy to get
/// wrong and impossible to notice: an anchor dir must be absolute, and the Debian-family
/// entry must use `.crt` - `update-ca-certificates` ignores anything else, which would
/// look like a successful install that never took.
#[test]
fn known_layouts_are_well_formed() {
    assert!(!KNOWN.is_empty());
    for store in KNOWN {
        match store {
            TrustStore::AnchorDir(s) => {
                assert!(s.anchor_dir.starts_with('/'), "{} anchor dir must be absolute", s.id);
                assert!(s.bundle.starts_with('/'), "{} bundle must be absolute", s.id);
                assert!(
                    !s.anchor_extension.starts_with('.'),
                    "{} extension is joined with a dot already",
                    s.id,
                );
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
            TrustStore::AnchorDir(a) if a.update_command == "update-ca-certificates" => Some(a),
            _ => None,
        })
        .expect("the ca-certificates layout is in the table");
    assert_eq!(
        debian.anchor_extension, "crt",
        "update-ca-certificates only processes .crt, so a .pem anchor would be ignored",
    );
}

#[test]
fn ids_are_unique() {
    let mut ids: Vec<&str> = KNOWN.iter().map(TrustStore::id).collect();
    ids.sort_unstable();
    let before = ids.len();
    ids.dedup();
    assert_eq!(before, ids.len(), "layout ids must be unique");
}

#[test]
fn kind_mirrors_the_variant() {
    for store in KNOWN {
        let expected = match store {
            TrustStore::AnchorDir(_) => TrustStoreKind::AnchorDir,
            TrustStore::Keychain(_) => TrustStoreKind::Keychain,
        };
        assert_eq!(store.kind(), expected);
    }
}

/// An explicit `--anchor-dir` asserts the directory model, whatever this box detects -
/// it is the escape hatch for a layout the table does not know.
#[test]
fn an_explicit_anchor_dir_selects_the_directory_model() {
    let resolved = resolve(
        Some(std::path::Path::new("/tmp/anchors")),
        Some("true"),
        Some(std::path::Path::new("/tmp/bundle.pem")),
    )
    .expect("fully overridden");
    assert_eq!(resolved.kind(), TrustStoreKind::AnchorDir);
    match resolved.placement {
        Placement::AnchorDir {
            dir,
            update_command,
            bundle,
            ..
        } => {
            assert_eq!(dir, std::path::Path::new("/tmp/anchors"));
            assert_eq!(update_command, "true");
            assert_eq!(bundle, std::path::Path::new("/tmp/bundle.pem"));
        }
        Placement::Keychain { .. } => panic!("an explicit anchor dir must not resolve to a keychain"),
    }
}
