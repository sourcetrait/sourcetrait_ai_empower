use crate::*;

pub(crate) struct VerifyPlan<'a> {
    /// Base whose `certs` leaf holds the installed material.
    pub secret_base: &'a Path,
    pub name: &'a str,
    pub store: &'a crate::store::Resolved,
}

pub(crate) struct Verified {
    pub key_path: PathBuf,
    pub trust_source: String,
    pub anchor_trusted: bool,
}

/// Close the install loop: did the anchor actually take, and can the unprivileged user
/// read its own key?
///
/// The key check READS the file rather than stating it, deliberately. `install` runs as
/// root and a stat succeeds regardless of ownership, so a stat would happily pass on a
/// root-owned key the MCP cannot open - precisely the failure this exists to catch, and
/// one that would otherwise surface much later as a channel-open error. Run this AS THE
/// INVOKING USER, never under sudo, or it proves nothing.
pub(crate) fn verify(plan: &VerifyPlan<'_>) -> Result<Verified> {
    let secret_dir = crate::generate::certs_dir(plan.secret_base);

    let key_path = secret_dir.join(format!("authority_{}.key.pem", plan.name));
    let key = fs::read_to_string(&key_path)
        .map_err(|e| CertError::io(format!("reading {}", key_path.display()), e))?;
    if !key.contains("PRIVATE KEY") {
        return Err(CertError::msg(format!(
            "{} is not a PEM private key",
            key_path.display(),
        )));
    }

    let anchor_path = secret_dir.join(format!("authority_{}.pem", plan.name));
    let anchor = fs::read_to_string(&anchor_path)
        .map_err(|e| CertError::io(format!("reading {}", anchor_path.display()), e))?;

    let (trust_source, bundle) = trust_bundle(&plan.store.placement)?;

    Ok(Verified {
        key_path,
        trust_source,
        anchor_trusted: bundle_contains(&bundle, &anchor),
    })
}

/// Every trusted certificate on this system, as PEM text, plus a label for where it
/// came from.
///
/// The two models differ only in HOW the text is obtained - a file on the anchor-dir
/// platforms, a `security` query on macOS - so the comparison below stays identical for
/// both rather than growing a second implementation.
fn trust_bundle(placement: &crate::store::Placement) -> Result<(String, String)> {
    match placement {
        crate::store::Placement::AnchorDir { bundle, .. } => {
            let text = fs::read_to_string(bundle)
                .map_err(|e| CertError::io(format!("reading {}", bundle.display()), e))?;
            Ok((bundle.display().to_string(), text))
        }
        crate::store::Placement::Keychain { program, keychain } => {
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

/// Does the trust store carry this certificate?
///
/// Compared on the base64 body with ALL whitespace stripped, because the store's copy
/// is regenerated rather than byte-copied: line wrapping and surrounding commentary
/// differ from the source anchor, so a literal substring test on the PEM would report a
/// false negative on a perfectly good install - the wrong direction for a verification
/// step to fail in.
pub(crate) fn bundle_contains(
    bundle: &str,
    anchor_pem: &str,
) -> bool {
    let body = pem_body(anchor_pem);
    if body.is_empty() {
        return false;
    }
    strip_whitespace(bundle).contains(&body)
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
