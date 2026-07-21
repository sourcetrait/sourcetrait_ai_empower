use crate::*;
use crate::generate::{CertFileKind, certs_dir};

#[test]
fn certs_leaf_is_appended_to_the_base() {
    assert_eq!(certs_dir(Path::new("/srv/grammar")), Path::new("/srv/grammar/certs"));
    assert_eq!(certs_dir(Path::new("rel")), Path::new("rel/certs"));
}

/// The authority / entity split is visible in the filenames on purpose: the public
/// authority pem is the only artifact that leaves for the system trust store, and
/// mixing it up with a key is the mistake worth making structurally obvious.
#[test]
fn artifact_names_follow_the_authority_entity_split() {
    let dir = Path::new("/tmp/x/certs");
    assert_eq!(
        CertFileKind::AuthorityPrivate.filepath(dir, "grammar"),
        Path::new("/tmp/x/certs/authority_grammar.key.pem"),
    );
    assert_eq!(
        CertFileKind::AuthorityPublic.filepath(dir, "grammar"),
        Path::new("/tmp/x/certs/authority_grammar.pem"),
    );
    assert_eq!(
        CertFileKind::EntityPrivate.filepath(dir, "grammar"),
        Path::new("/tmp/x/certs/entity_grammar.key.pem"),
    );
    assert_eq!(
        CertFileKind::EntityPublic.filepath(dir, "grammar"),
        Path::new("/tmp/x/certs/entity_grammar.pem"),
    );
}

#[test]
fn cert_files_reports_absence() {
    let files = CertFiles::new(Path::new("/nonexistent/certs"), "grammar");
    assert!(!files.exist());
    assert_eq!(files.all().len(), 4);
}
