use crate::*;
use crate::config::CertGenConfig;

const FULL: &str = r#"
name = "grammar"
organization = "SourceTrait"
validity_days = 365

[authority]
common_name = "SourceTrait Grammar Local CA"

[entity]
common_name = "localhost"
subject_alt_names = ["127.0.0.1", "::1", "localhost"]
"#;

fn parse(text: &str) -> Result<CertGenConfig> {
    CertGenConfig::parse(text, Path::new("certgen.toml"))
}

#[test]
fn parses_a_full_config() {
    let c = parse(FULL).expect("valid config");
    assert_eq!(c.name, "grammar");
    assert_eq!(c.organization, "SourceTrait");
    assert_eq!(c.validity_days, 365);
    assert_eq!(c.authority.common_name, "SourceTrait Grammar Local CA");
    assert_eq!(c.entity.common_name, "localhost");
    assert_eq!(c.entity.subject_alt_names.len(), 3);
}

#[test]
fn validity_days_defaults() {
    let text = FULL.replace("validity_days = 365\n", "");
    assert_eq!(parse(&text).expect("valid").validity_days, 3650);
}

#[test]
fn rejects_empty_subject_alt_names() {
    let text = FULL.replace(
        r#"subject_alt_names = ["127.0.0.1", "::1", "localhost"]"#,
        "subject_alt_names = []",
    );
    assert!(parse(&text).is_err(), "an empty SAN list must be rejected");
}

#[test]
fn rejects_nonpositive_validity() {
    let text = FULL.replace("validity_days = 365", "validity_days = 0");
    assert!(parse(&text).is_err());
}

/// A typo in a parameter file that silently does nothing is worse than a hard failure,
/// so the schema is closed rather than open.
#[test]
fn rejects_unknown_fields() {
    let text = format!("{FULL}\nvalidity_dayz = 30\n");
    assert!(parse(&text).is_err(), "an unknown key must be rejected");
}

#[test]
fn classifies_ip_and_dns_sans() {
    let sans = parse(FULL).expect("valid").subject_alt_names().expect("sans");
    assert!(matches!(sans[0], rcgen::SanType::IpAddress(_)), "127.0.0.1 is an IP");
    assert!(matches!(sans[1], rcgen::SanType::IpAddress(_)), "::1 is an IP");
    assert!(matches!(sans[2], rcgen::SanType::DnsName(_)), "localhost is a DNS name");
}

/// The SAN list is what hostname verification actually reads, so an entry that cannot
/// be encoded must fail at parse rather than produce a leaf that silently omits it.
#[test]
fn rejects_non_ascii_dns_name() {
    let text = FULL.replace(
        r#"subject_alt_names = ["127.0.0.1", "::1", "localhost"]"#,
        r#"subject_alt_names = ["héllo.example"]"#,
    );
    let config = parse(&text).expect("parses; the name is only rejected when encoded");
    assert!(config.subject_alt_names().is_err(), "a non-ASCII dNSName must fail");
}

#[test]
fn rejects_empty_san_entry() {
    let text = FULL.replace(
        r#"subject_alt_names = ["127.0.0.1", "::1", "localhost"]"#,
        r#"subject_alt_names = ["  "]"#,
    );
    let config = parse(&text).expect("parses");
    assert!(config.subject_alt_names().is_err());
}
