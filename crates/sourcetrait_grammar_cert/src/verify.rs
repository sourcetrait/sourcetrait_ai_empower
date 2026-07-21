use crate::*;

use crate::store::TrustSource;

pub(crate) struct VerifyPlan<'a> {
    /// The secret DATA home whose `sourcetrait/grammar/certs` leaf holds the material.
    pub secret_data: &'a Path,
    pub name: &'a str,
    pub source: &'a TrustSource,
    /// The CA cert to look for. Defaults to the installed copy, but can be given so a
    /// manually-placed CA still verifies.
    pub ca_cert: Option<&'a Path>,
}

pub(crate) struct Verified {
    pub keys_read: Vec<PathBuf>,
    pub trust_source: String,
    pub ca_trusted: bool,
}

/// Did the CA actually become trusted, and can the unprivileged user read the keys it
/// needs at runtime?
///
/// The key check READS rather than stats: `install` runs as root and a stat succeeds
/// regardless of ownership, so a stat would pass on a root-owned key the host cannot
/// open. Run this AS THE INVOKING USER, never under sudo, or it proves nothing.
pub(crate) fn verify(plan: &VerifyPlan<'_>) -> Result<Verified> {
    let secret_dir = crate::generate::secret_certs_dir(plan.secret_data);

    // BOTH private keys. The server serves TLS with the ENTITY key, so checking only
    // the authority key would pass while the key actually used at runtime was
    // unreadable.
    let mut keys_read = Vec::new();
    for key_name in [
        format!("authority_{}.key.pem", plan.name),
        format!("entity_{}.key.pem", plan.name),
    ] {
        let path = secret_dir.join(&key_name);
        let text = fs::read_to_string(&path)
            .map_err(|e| CertError::io(format!("reading {}", path.display()), e))?;
        if !text.contains("PRIVATE KEY") {
            return Err(CertError::msg(format!(
                "{} is not a PEM private key",
                path.display(),
            )));
        }
        keys_read.push(path);
    }

    let ca_path = plan
        .ca_cert
        .map(Path::to_path_buf)
        .unwrap_or_else(|| secret_dir.join(format!("authority_{}.pem", plan.name)));
    let ca = fs::read_to_string(&ca_path)
        .map_err(|e| CertError::io(format!("reading {}", ca_path.display()), e))?;

    let (trust_source, trusted_set) = read_trusted_set(plan.source)?;

    Ok(Verified {
        keys_read,
        trust_source,
        ca_trusted: contains_cert(&trusted_set, &ca),
    })
}

/// Every trusted certificate as PEM text, plus a label for where it came from. The two
/// models differ only in HOW the text is obtained, so the comparison stays shared.
fn read_trusted_set(source: &TrustSource) -> Result<(String, String)> {
    match source {
        TrustSource::Bundle(path) => {
            let text = fs::read_to_string(path)
                .map_err(|e| CertError::io(format!("reading {}", path.display()), e))?;
            Ok((path.display().to_string(), text))
        }
        TrustSource::Keychain { program, keychain } => {
            let key = keychain.to_string_lossy().into_owned();
            let output = process::Command::new(program)
                .args(["find-certificate", "-a", "-p", &key])
                .output()
                .map_err(|e| CertError::io(format!("running {program}"), e))?;
            if !output.status.success() {
                return Err(CertError::Command {
                    command: format!("{program} find-certificate"),
                    status: output.status.to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                });
            }
            Ok((
                keychain.display().to_string(),
                String::from_utf8_lossy(&output.stdout).into_owned(),
            ))
        }
    }
}

/// Compared on the base64 body with all whitespace stripped: the store's copy is
/// regenerated rather than byte-copied, so wrapping and commentary differ and a literal
/// PEM substring test would report a false negative on a good install.
pub(crate) fn contains_cert(
    trusted_set: &str,
    cert_pem: &str,
) -> bool {
    let body = pem_body(cert_pem);
    if body.is_empty() {
        return false;
    }
    strip_whitespace(trusted_set).contains(&body)
}

pub(crate) fn pem_body(pem: &str) -> String {
    let inner: String = pem
        .lines()
        .skip_while(|l| !l.starts_with("-----BEGIN"))
        .skip(1)
        .take_while(|l| !l.starts_with("-----END"))
        .collect();
    strip_whitespace(&inner)
}

fn strip_whitespace(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
