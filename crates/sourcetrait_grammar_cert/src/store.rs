use crate::*;

/// How a platform ingests a CA certificate.
///
/// A fieldful enum because the MODELS differ, not the paths: a Linux store is a
/// directory you drop a cert into plus a refresh command; macOS hands it to `security`
/// and has no such directory.
pub(crate) enum TrustStore {
    TrustDir(TrustDirStore),
    Keychain(KeychainStore),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrustStoreKind {
    TrustDir,
    Keychain,
}

impl TrustStore {
    pub(crate) fn kind(&self) -> TrustStoreKind {
        match self {
            Self::TrustDir(_) => TrustStoreKind::TrustDir,
            Self::Keychain(_) => TrustStoreKind::Keychain,
        }
    }

    pub(crate) fn id(&self) -> &'static str {
        match self {
            Self::TrustDir(s) => s.id,
            Self::Keychain(s) => s.id,
        }
    }

    /// The same existence check that stops us writing blindly identifies the platform.
    fn present(&self) -> bool {
        match self {
            Self::TrustDir(s) => Path::new(s.trust_dir).is_dir(),
            Self::Keychain(s) => {
                Path::new(s.program).is_file() && Path::new(s.keychain).exists()
            }
        }
    }
}

pub(crate) struct TrustDirStore {
    pub id: &'static str,
    pub trust_dir: &'static str,
    /// `update-ca-certificates` only processes `.crt`, so the extension is per-store.
    pub cert_extension: &'static str,
    pub update_command: &'static str,
    /// The extracted bundle `verify` reads back.
    pub bundle: &'static str,
}

pub(crate) struct KeychainStore {
    pub id: &'static str,
    /// Absolute, because this doubles as the presence check.
    pub program: &'static str,
    pub keychain: &'static str,
}

pub(crate) const KNOWN: &[TrustStore] = &[
    TrustStore::TrustDir(TrustDirStore {
        id: "pki-ca-trust",
        trust_dir: "/etc/pki/ca-trust/source/anchors",
        cert_extension: "pem",
        update_command: "update-ca-trust",
        bundle: "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",
    }),
    TrustStore::TrustDir(TrustDirStore {
        id: "ca-certificates-trust-source",
        trust_dir: "/etc/ca-certificates/trust-source/anchors",
        cert_extension: "pem",
        update_command: "update-ca-trust",
        bundle: "/etc/ssl/certs/ca-certificates.crt",
    }),
    TrustStore::TrustDir(TrustDirStore {
        id: "ca-certificates",
        trust_dir: "/usr/local/share/ca-certificates",
        cert_extension: "crt",
        update_command: "update-ca-certificates",
        bundle: "/etc/ssl/certs/ca-certificates.crt",
    }),
    TrustStore::Keychain(KeychainStore {
        id: "macos-system-keychain",
        program: "/usr/bin/security",
        keychain: "/Library/Keychains/System.keychain",
    }),
];

pub(crate) fn detect() -> Option<&'static TrustStore> {
    KNOWN.iter().find(|s| s.present())
}

/// Where `install` puts the CA cert. Carries only what installing needs.
pub(crate) enum TrustTarget {
    Dir {
        dir: PathBuf,
        extension: String,
        update_command: String,
    },
    Keychain {
        program: String,
        keychain: PathBuf,
    },
}

/// Where `verify` reads the trusted set from. Carries only what verifying needs.
pub(crate) enum TrustSource {
    Bundle(PathBuf),
    Keychain {
        program: String,
        keychain: PathBuf,
    },
}

pub(crate) fn kind_of_target(target: &TrustTarget) -> TrustStoreKind {
    match target {
        TrustTarget::Dir { .. } => TrustStoreKind::TrustDir,
        TrustTarget::Keychain { .. } => TrustStoreKind::Keychain,
    }
}

/// Resolve the install target. Each command resolves ONLY its own fields - an earlier
/// cut shared one resolver, so `install --trust-dir` demanded a `--bundle` it does not
/// expose and could not be run at all on a keychain platform.
pub(crate) fn resolve_target(
    trust_dir: Option<&Path>,
    update_command: Option<&str>,
) -> Result<(TrustTarget, Option<&'static str>)> {
    let store = detect();
    let detected_dir = match store {
        Some(TrustStore::TrustDir(s)) => Some(s),
        _ => None,
    };

    if let Some(dir) = trust_dir {
        let update_command = update_command
            .map(str::to_string)
            .or_else(|| detected_dir.map(|s| s.update_command.to_string()))
            .ok_or_else(|| missing("--update-command"))?;
        return Ok((
            TrustTarget::Dir {
                dir: dir.to_path_buf(),
                extension: detected_dir.map(|s| s.cert_extension).unwrap_or("pem").to_string(),
                update_command,
            },
            store.map(TrustStore::id),
        ));
    }

    match store {
        Some(TrustStore::TrustDir(s)) => Ok((
            TrustTarget::Dir {
                dir: PathBuf::from(s.trust_dir),
                extension: s.cert_extension.to_string(),
                update_command: update_command.unwrap_or(s.update_command).to_string(),
            },
            Some(s.id),
        )),
        Some(TrustStore::Keychain(s)) => Ok((
            TrustTarget::Keychain {
                program: s.program.to_string(),
                keychain: PathBuf::from(s.keychain),
            },
            Some(s.id),
        )),
        None => Err(missing("--trust-dir")),
    }
}

/// Resolve where the trusted set is read from. A `--bundle` on a keychain platform is
/// an ERROR rather than silently dropped.
pub(crate) fn resolve_source(bundle: Option<&Path>) -> Result<TrustSource> {
    let store = detect();
    if let Some(path) = bundle {
        if let Some(TrustStore::Keychain(_)) = store {
            return Err(CertError::msg(
                "--bundle does not apply to a keychain trust store",
            ));
        }
        return Ok(TrustSource::Bundle(path.to_path_buf()));
    }
    match store {
        Some(TrustStore::TrustDir(s)) => Ok(TrustSource::Bundle(PathBuf::from(s.bundle))),
        Some(TrustStore::Keychain(s)) => Ok(TrustSource::Keychain {
            program: s.program.to_string(),
            keychain: PathBuf::from(s.keychain),
        }),
        None => Err(missing("--bundle")),
    }
}

fn missing(flag: &str) -> CertError {
    CertError::msg(format!("no known trust store found; pass {flag}"))
}
