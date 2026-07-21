use crate::*;

pub(crate) struct VerifyPlan<'a> {
    /// Base whose `certs` leaf holds the installed key material.
    pub secret_base: &'a Path,
    pub name: &'a str,
    /// System trust anchor dir, used verbatim.
    pub anchor_dir: &'a Path,
    /// The extracted trust bundle `update-ca-trust` rebuilds.
    pub bundle: &'a Path,
}

pub(crate) struct Verified {
    pub key_path: PathBuf,
    pub anchor_path: PathBuf,
    pub anchor_in_bundle: bool,
}

/// Close the install loop: did the anchor actually take, and can the unprivileged user
/// read its own key?
///
/// The key check READS the file rather than stating it, deliberately. `install` runs as
/// root and a stat succeeds regardless of ownership, so a stat would happily pass on a
/// root-owned key that the MCP cannot open - which is precisely the failure this is
/// here to catch, and it would otherwise surface much later as a channel-open error.
/// Run this AS THE INVOKING USER, never under sudo, or it proves nothing.
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

    let anchor_path = plan.anchor_dir.join(format!("{}.pem", plan.name));
    let anchor = fs::read_to_string(&anchor_path).map_err(|e| {
        CertError::io(
            format!("reading the installed anchor {}", anchor_path.display()),
            e,
        )
    })?;

    let bundle = fs::read_to_string(plan.bundle).map_err(|e| {
        CertError::io(format!("reading the trust bundle {}", plan.bundle.display()), e)
    })?;

    Ok(Verified {
        key_path,
        anchor_path,
        anchor_in_bundle: bundle_contains(&bundle, &anchor),
    })
}

/// Does the extracted bundle carry this certificate?
///
/// Compared on the base64 body with ALL whitespace stripped, because the bundle is
/// regenerated rather than copied: line wrapping and the surrounding commentary differ
/// from the source anchor, so a literal substring test on the PEM would report a false
/// negative on a perfectly good install.
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
