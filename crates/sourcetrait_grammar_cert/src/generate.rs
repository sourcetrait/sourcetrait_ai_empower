use crate::*;

/// The four artifacts a generate run emits, all into ONE directory.
///
/// `generate` knows nothing about destinations on purpose - `install` owns every
/// placement decision, so there is exactly one place to look when asking where a file
/// ends up rather than the logic being smeared across both halves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CertFileKind {
    AuthorityPrivate,
    AuthorityPublic,
    EntityPrivate,
    EntityPublic,
}

impl CertFileKind {
    pub(crate) fn filepath(
        &self,
        dir: &Path,
        name: &str,
    ) -> PathBuf {
        match self {
            Self::AuthorityPrivate => dir.join(format!("authority_{name}.key.pem")),
            Self::AuthorityPublic => dir.join(format!("authority_{name}.pem")),
            Self::EntityPrivate => dir.join(format!("entity_{name}.key.pem")),
            Self::EntityPublic => dir.join(format!("entity_{name}.pem")),
        }
    }
}

/// Every path argument this tool takes names a PARENT; we own the `certs` leaf under
/// it. So a caller passes the location it wants involved and never has to know our
/// filenames, and the same base can be handed to `generate`, `install` and `verify`
/// without any of them meaning something subtly different by it.
///
/// The one place this does NOT apply is the system trust anchor dir, which has its own
/// contract with `update-ca-trust` - a `certs` subdir there would simply not be picked
/// up. That path is used exactly as given.
pub(crate) fn certs_dir(base: &Path) -> PathBuf {
    base.join("certs")
}

/// The install destination inside a SHARED root: `<secret_data>/sourcetrait/grammar/certs`.
///
/// The vendor + app segments are not decoration. `$XDGX_SECRET_DATA_HOME` belongs to the
/// box, not to us, so dropping a bare `certs` dir at its root would collide with every
/// other application that ever wants one. Same shape as the store's own keypair under
/// `$XDG_DATA_HOME/sourcetrait/grammar/`, and the segments come from `lib_grammar` so
/// the two cannot disagree about where the vendor prefix is.
pub(crate) fn secret_certs_dir(secret_data: &Path) -> PathBuf {
    secret_data
        .join(lib_grammar::consts::SOURCETRAIT)
        .join(lib_grammar::consts::GRAMMAR)
        .join("certs")
}

pub(crate) struct CertFiles {
    pub(crate) authority_private: PathBuf,
    pub(crate) authority_public: PathBuf,
    pub(crate) entity_private: PathBuf,
    pub(crate) entity_public: PathBuf,
}

impl CertFiles {
    pub(crate) fn new(
        certs_dir: &Path,
        cert_name: &str,
    ) -> Self {
        Self {
            authority_private: CertFileKind::AuthorityPrivate.filepath(certs_dir, cert_name),
            authority_public: CertFileKind::AuthorityPublic.filepath(certs_dir, cert_name),
            entity_private: CertFileKind::EntityPrivate.filepath(certs_dir, cert_name),
            entity_public: CertFileKind::EntityPublic.filepath(certs_dir, cert_name),
        }
    }

    pub(crate) fn exist(&self) -> bool {
        self.authority_private.exists()
            && self.authority_public.exists()
            && self.entity_private.exists()
            && self.entity_public.exists()
    }

    pub(crate) fn all(&self) -> [&PathBuf; 4] {
        [
            &self.authority_private,
            &self.authority_public,
            &self.entity_private,
            &self.entity_public,
        ]
    }

    /// Takes the four PEM strings rather than the live rcgen values.
    ///
    /// The reference implementation needed a self-referencing struct here, because
    /// `CertifiedIssuer<'this, &'this KeyPair>` borrows the keypair and the writer
    /// wanted both afterwards. Generation, signing and writing all happen in one
    /// scope, so taking the already-serialized PEMs removes the self-reference - and
    /// with it a proc-macro dependency, which matters for a binary whose whole
    /// justification is a small blast radius.
    fn write(
        &self,
        authority_key_pem: &str,
        authority_cert_pem: &str,
        entity_key_pem: &str,
        entity_cert_pem: &str,
    ) -> Result<()> {
        for (path, contents) in [
            (&self.authority_private, authority_key_pem),
            (&self.authority_public, authority_cert_pem),
            (&self.entity_private, entity_key_pem),
            (&self.entity_public, entity_cert_pem),
        ] {
            fs::write(path, contents)
                .map_err(|e| CertError::io(format!("writing {}", path.display()), e))?;
        }
        restrict(&self.authority_private)?;
        restrict(&self.entity_private)?;
        Ok(())
    }
}

/// Private keys go 0600. The staging dir is already 0700, but a key that is readable
/// the moment someone widens the directory is a latent mistake worth closing here.
fn restrict(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|e| CertError::io(format!("stat {}", path.display()), e))?
        .permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms)
        .map_err(|e| CertError::io(format!("chmod 600 {}", path.display()), e))
}

/// Mint a self-signed CA and a leaf it signs, into `<base>/certs`.
///
/// An EXISTING `certs` dir is a hard error, not a no-op. The reference guarded on "all
/// four artifacts present" and skipped, which is the dangerous shape: a partially
/// populated dir would regenerate over live material, and a fully populated one would
/// silently do nothing while the caller believed it had fresh certs. Refusing makes the
/// operator decide, and neither a stale authority nor a rotated-out-from-under-you
/// anchor can happen by accident.
pub(crate) fn generate(
    config: &CertGenConfig,
    base: &Path,
) -> Result<CertFiles> {
    let out_dir = certs_dir(base);
    if out_dir.exists() {
        return Err(CertError::CertsDirExists {
            path: out_dir.display().to_string(),
        });
    }
    let files = CertFiles::new(&out_dir, &config.name);

    create_staging_dir(&out_dir)?;

    let (not_before, not_after) = validity_window(config.validity_days);

    let authority_keypair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
        .map_err(|e| CertError::rcgen("generating the authority keypair", e))?;

    let mut authority_params = rcgen::CertificateParams::default();
    authority_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    authority_params.key_usages.extend([
        rcgen::KeyUsagePurpose::DigitalSignature,
        rcgen::KeyUsagePurpose::KeyCertSign,
    ]);
    authority_params
        .distinguished_name
        .push(rcgen::DnType::CommonName, config.authority.common_name.as_str());
    authority_params
        .distinguished_name
        .push(rcgen::DnType::OrganizationName, config.organization.as_str());
    authority_params.not_before = not_before;
    authority_params.not_after = not_after;

    let issuer = rcgen::CertifiedIssuer::self_signed(authority_params, &authority_keypair)
        .map_err(|e| CertError::rcgen("self-signing the authority certificate", e))?;

    let entity_keypair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
        .map_err(|e| CertError::rcgen("generating the entity keypair", e))?;

    let mut entity_params = rcgen::CertificateParams::default();
    entity_params.is_ca = rcgen::IsCa::NoCa;
    // Without this the chain does not build for some verifiers; easy to omit and
    // tedious to diagnose, so it is kept from the reference deliberately.
    entity_params.use_authority_key_identifier_extension = true;
    entity_params
        .key_usages
        .push(rcgen::KeyUsagePurpose::DigitalSignature);
    entity_params
        .extended_key_usages
        .push(rcgen::ExtendedKeyUsagePurpose::ServerAuth);
    entity_params
        .distinguished_name
        .push(rcgen::DnType::CommonName, config.entity.common_name.as_str());
    entity_params
        .distinguished_name
        .push(rcgen::DnType::OrganizationName, config.organization.as_str());
    entity_params.subject_alt_names = config.subject_alt_names()?;
    entity_params.not_before = not_before;
    entity_params.not_after = not_after;

    let entity_cert = entity_params
        .signed_by(&entity_keypair, &issuer)
        .map_err(|e| CertError::rcgen("signing the entity certificate", e))?;

    files.write(
        &authority_keypair.serialize_pem(),
        &issuer.pem(),
        &entity_keypair.serialize_pem(),
        &entity_cert.pem(),
    )?;
    Ok(files)
}

/// ECDSA P-256 for both keys, kept from the reference. It is the conservative pick for
/// client compatibility, where ed25519 would have been a gamble against whatever TLS
/// stack the peer happens to ship.
fn validity_window(days: i64) -> (time::OffsetDateTime, time::OffsetDateTime) {
    let now = time::OffsetDateTime::now_utc();
    (now, now + time::Duration::days(days))
}

/// The staging dir holds CA PRIVATE KEY material, so it is created 0700 explicitly
/// rather than inheriting whatever umask the caller happened to have.
fn create_staging_dir(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if !dir.exists() {
        fs::create_dir_all(dir)
            .map_err(|e| CertError::io(format!("creating {}", dir.display()), e))?;
    }
    let mut perms = fs::metadata(dir)
        .map_err(|e| CertError::io(format!("stat {}", dir.display()), e))?
        .permissions();
    perms.set_mode(0o700);
    fs::set_permissions(dir, perms)
        .map_err(|e| CertError::io(format!("chmod 700 {}", dir.display()), e))
}
