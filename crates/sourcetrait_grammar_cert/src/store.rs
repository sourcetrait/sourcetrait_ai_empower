use crate::*;

/// How a platform ingests a trust anchor.
///
/// Two genuinely different MODELS, which is why this is a fieldful enum rather than a
/// table of paths. A Linux trust store is a DIRECTORY you drop a file into followed by
/// a refresh command; macOS has no such directory - it hands the certificate to
/// `security`, which puts it in a keychain. No amount of changing `--anchor-dir` turns
/// one into the other.
///
/// This is not a portability promise. We run on one box. It is here because picking one
/// distribution's layout as THE default would be an arbitrary choice with no upside,
/// and because macOS is a real development path even though it will never run a server.
pub(crate) enum TrustStore {
    AnchorDir(AnchorDirStore),
    Keychain(KeychainStore),
}

/// The Copy discriminant, for naming a model without its payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrustStoreKind {
    AnchorDir,
    Keychain,
}

impl TrustStore {
    pub(crate) fn kind(&self) -> TrustStoreKind {
        match self {
            Self::AnchorDir(_) => TrustStoreKind::AnchorDir,
            Self::Keychain(_) => TrustStoreKind::Keychain,
        }
    }

    pub(crate) fn id(&self) -> &'static str {
        match self {
            Self::AnchorDir(s) => s.id,
            Self::Keychain(s) => s.id,
        }
    }

    /// Is this the layout actually present on the running system? The same existence
    /// check that stops us writing blindly is what identifies the platform.
    fn present(&self) -> bool {
        match self {
            Self::AnchorDir(s) => Path::new(s.anchor_dir).is_dir(),
            Self::Keychain(s) => {
                Path::new(s.program).is_file() && Path::new(s.keychain).exists()
            }
        }
    }
}

pub(crate) struct AnchorDirStore {
    pub id: &'static str,
    pub anchor_dir: &'static str,
    /// `update-ca-certificates` only processes files ending `.crt`, so the extension
    /// belongs to the store. This is why "just change the path" does not move you
    /// between layouts.
    pub anchor_extension: &'static str,
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

/// The layouts we recognize, most-specific first.
pub(crate) const KNOWN: &[TrustStore] = &[
    TrustStore::AnchorDir(AnchorDirStore {
        id: "pki-ca-trust",
        anchor_dir: "/etc/pki/ca-trust/source/anchors",
        anchor_extension: "pem",
        update_command: "update-ca-trust",
        bundle: "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",
    }),
    TrustStore::AnchorDir(AnchorDirStore {
        id: "ca-certificates-trust-source",
        anchor_dir: "/etc/ca-certificates/trust-source/anchors",
        anchor_extension: "pem",
        update_command: "update-ca-trust",
        bundle: "/etc/ssl/certs/ca-certificates.crt",
    }),
    TrustStore::AnchorDir(AnchorDirStore {
        id: "ca-certificates",
        anchor_dir: "/usr/local/share/ca-certificates",
        anchor_extension: "crt",
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

/// The layout `install` / `verify` will actually use, after any explicit overrides.
pub(crate) struct Resolved {
    pub placement: Placement,
    /// Which known layout was detected, if any.
    pub detected: Option<&'static str>,
}

pub(crate) enum Placement {
    AnchorDir {
        dir: PathBuf,
        extension: String,
        update_command: String,
        bundle: PathBuf,
    },
    Keychain {
        program: String,
        keychain: PathBuf,
    },
}

impl Resolved {
    pub(crate) fn kind(&self) -> TrustStoreKind {
        match self.placement {
            Placement::AnchorDir { .. } => TrustStoreKind::AnchorDir,
            Placement::Keychain { .. } => TrustStoreKind::Keychain,
        }
    }
}

/// Overrides apply per field on top of the detected layout. Passing `--anchor-dir`
/// selects the directory model explicitly, which is also the escape hatch for a layout
/// we do not know.
pub(crate) fn resolve(
    anchor_dir: Option<&Path>,
    update_command: Option<&str>,
    bundle: Option<&Path>,
) -> Result<Resolved> {
    let store = detect();

    // An explicit --anchor-dir means the caller is asserting the directory model, even
    // on a box where detection found something else.
    if let Some(dir) = anchor_dir {
        let detected_dir = match store {
            Some(TrustStore::AnchorDir(s)) => Some(s),
            _ => None,
        };
        return Ok(Resolved {
            placement: Placement::AnchorDir {
                dir: dir.to_path_buf(),
                extension: detected_dir.map(|s| s.anchor_extension).unwrap_or("pem").to_string(),
                update_command: update_command
                    .map(str::to_string)
                    .or_else(|| detected_dir.map(|s| s.update_command.to_string()))
                    .ok_or_else(|| undetected("--update-command"))?,
                bundle: bundle
                    .map(Path::to_path_buf)
                    .or_else(|| detected_dir.map(|s| PathBuf::from(s.bundle)))
                    .ok_or_else(|| undetected("--bundle"))?,
            },
            detected: store.map(TrustStore::id),
        });
    }

    match store {
        Some(TrustStore::AnchorDir(s)) => Ok(Resolved {
            placement: Placement::AnchorDir {
                dir: PathBuf::from(s.anchor_dir),
                extension: s.anchor_extension.to_string(),
                update_command: update_command.unwrap_or(s.update_command).to_string(),
                bundle: bundle
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from(s.bundle)),
            },
            detected: Some(s.id),
        }),
        Some(TrustStore::Keychain(s)) => Ok(Resolved {
            placement: Placement::Keychain {
                program: s.program.to_string(),
                keychain: PathBuf::from(s.keychain),
            },
            detected: Some(s.id),
        }),
        None => Err(undetected("--anchor-dir")),
    }
}

fn undetected(flag: &str) -> CertError {
    CertError::msg(format!("no known trust store found; pass {flag}"))
}
