use crate::*;

use crate::store::{TrustTarget, kind_of_target};

pub(crate) struct InstallPlan<'a> {
    /// The base `generate` was given; its artifacts are in `<base>/certs`.
    pub cert_base: &'a Path,
    pub name: &'a str,
    pub target: &'a TrustTarget,
    /// The secret DATA home; key material lands in `<it>/sourcetrait/grammar/certs`.
    pub secret_data: &'a Path,
    /// User to hand the key material to. Defaults to `$SUDO_USER`.
    pub owner: Option<&'a str>,
}

pub(crate) struct Installed {
    /// Where the CA ended up: a file, or the keychain that ingested it.
    pub trusted_at: PathBuf,
    pub secrets: Vec<PathBuf>,
    pub owner: Option<String>,
}

/// Place the key material, then make the system trust the CA.
///
/// Ownership is not housekeeping: under `sudo` everything we create is root-owned, and
/// leaving it that way means the unprivileged host cannot read the key it is supposed
/// to own - surfacing much later as a channel-open failure.
pub(crate) fn install(plan: &InstallPlan<'_>) -> Result<Installed> {
    // PREFLIGHT: every check before any mutation.
    let source_dir = crate::generate::certs_dir(plan.cert_base);
    let files = CertFiles::new(&source_dir, plan.name);
    if !files.exist() {
        return Err(CertError::NotACertDir {
            path: source_dir.display().to_string(),
            reason: format!("missing `{}` artifacts", plan.name),
        });
    }

    // The trust dir must ALREADY EXIST. Creating it would turn a typo into a silent
    // success - a cert in a directory nothing reads.
    if let TrustTarget::Dir { dir, .. } = plan.target {
        require_existing_dir(dir, "trust dir")?;
    }
    require_existing_dir(plan.secret_data, "secret data dir")?;

    let secret_dir = crate::generate::secret_certs_dir(plan.secret_data);
    if secret_dir.exists() {
        return Err(CertError::CertsDirExists {
            path: secret_dir.display().to_string(),
        });
    }
    let owner = resolve_owner(plan.owner)?;

    // ACT. Key material first - it is the half we can roll back.
    let created = create_private_chain(plan.secret_data, &secret_dir, &owner)?;
    let mut secrets = Vec::new();
    // The CA PUBLIC cert rides along so `verify` has the body to look for, whichever
    // model took the trust.
    for source in [
        &files.authority_private,
        &files.authority_public,
        &files.entity_private,
        &files.entity_public,
    ] {
        let name = source
            .file_name()
            .ok_or_else(|| CertError::msg("a cert artifact has no file name"))?;
        let dest = secret_dir.join(name);
        if let Err(e) = copy(source, &dest) {
            rollback(&created);
            return Err(e);
        }
        if let Some((_, uid, gid)) = &owner
            && let Err(e) = chown_to(&dest, *uid, *gid)
        {
            rollback(&created);
            return Err(e);
        }
        secrets.push(dest);
    }

    // Roll the key material back if the trust step fails, so a failed refresh leaves
    // nothing behind and the command can simply be re-run.
    let trusted_at = match trust_ca(plan.target, &files.authority_public, plan.name) {
        Ok(p) => p,
        Err(e) => {
            rollback(&created);
            return Err(e);
        }
    };

    Ok(Installed {
        trusted_at,
        secrets,
        owner: owner.map(|(user, _, _)| user),
    })
}

/// Create each level of `<secret_data>/sourcetrait/grammar/certs` explicitly at 0700
/// and hand each to the owner.
///
/// `create_dir_all` takes its mode from the umask, so under root's usual 022 every
/// level came out 0755 - a world-traversable directory holding a CA key - and only the
/// leaf was ever chowned, leaving root-owned dirs in someone else's secret tree.
/// Returns the levels we created, newest last, for rollback.
fn create_private_chain(
    secret_data: &Path,
    secret_dir: &Path,
    owner: &Option<(String, u32, u32)>,
) -> Result<Vec<PathBuf>> {
    use std::os::unix::fs::PermissionsExt;

    let relative = secret_dir
        .strip_prefix(secret_data)
        .map_err(|_| CertError::msg("secret dir is not under the secret data home"))?;

    let mut created = Vec::new();
    let mut path = secret_data.to_path_buf();
    for component in relative.components() {
        path = path.join(component);
        if path.exists() {
            continue;
        }
        if let Err(e) = fs::create_dir(&path) {
            rollback(&created);
            return Err(CertError::io(format!("creating {}", path.display()), e));
        }
        created.push(path.clone());

        let mut perms = match fs::metadata(&path) {
            Ok(m) => m.permissions(),
            Err(e) => {
                rollback(&created);
                return Err(CertError::io(format!("stat {}", path.display()), e));
            }
        };
        perms.set_mode(0o700);
        if let Err(e) = fs::set_permissions(&path, perms) {
            rollback(&created);
            return Err(CertError::io(format!("chmod 700 {}", path.display()), e));
        }
        if let Some((_, uid, gid)) = owner
            && let Err(e) = chown_to(&path, *uid, *gid)
        {
            rollback(&created);
            return Err(e);
        }
    }
    Ok(created)
}

/// Remove what we created, deepest first. Best-effort: a rollback failure must not mask
/// the error that triggered it.
fn rollback(created: &[PathBuf]) {
    for path in created.iter().rev() {
        let _ = fs::remove_dir_all(path);
    }
}

fn require_existing_dir(
    path: &Path,
    what: &str,
) -> Result<()> {
    if !path.exists() {
        return Err(CertError::MissingDir {
            what: what.to_string(),
            path: path.display().to_string(),
        });
    }
    if !path.is_dir() {
        return Err(CertError::NotADir {
            what: what.to_string(),
            path: path.display().to_string(),
        });
    }
    Ok(())
}

/// `$SUDO_USER` when the caller did not name one. Absent means a non-sudo run, where
/// the files already belong to the right user.
fn resolve_owner(explicit: Option<&str>) -> Result<Option<(String, u32, u32)>> {
    let name = match explicit {
        Some(n) => n.to_string(),
        None => match std::env::var("SUDO_USER") {
            Ok(n) if !n.is_empty() => n,
            _ => return Ok(None),
        },
    };
    let user = nix::unistd::User::from_name(&name)
        .map_err(|e| CertError::UnknownUser {
            user: name.clone(),
            reason: e.to_string(),
        })?
        .ok_or_else(|| CertError::UnknownUser {
            user: name.clone(),
            reason: "no such user in the password database".to_string(),
        })?;
    Ok(Some((name, user.uid.as_raw(), user.gid.as_raw())))
}

fn chown_to(
    path: &Path,
    uid: u32,
    gid: u32,
) -> Result<()> {
    nix::unistd::chown(
        path,
        Some(nix::unistd::Uid::from_raw(uid)),
        Some(nix::unistd::Gid::from_raw(gid)),
    )
    .map_err(|source| CertError::Chown {
        path: path.display().to_string(),
        uid,
        source,
    })
}

/// Copy, creating NOTHING. Directories are either ours to make deliberately or must
/// already exist.
fn copy(
    from: &Path,
    to: &Path,
) -> Result<()> {
    fs::copy(from, to).map_err(|e| {
        CertError::io(format!("copying {} to {}", from.display(), to.display()), e)
    })?;
    Ok(())
}

/// The filename we drop into a SHARED system trust dir, vendor-prefixed.
///
/// A bare `<name>.pem` there is the same mistake as a bare `certs/` in the secret data
/// home: the directory belongs to the box, so an unqualified name both collides and
/// says nothing about who installed it.
pub(crate) fn trust_file_name(
    name: &str,
    extension: &str,
) -> String {
    format!("{}_{name}.{extension}", lib_grammar::consts::SOURCETRAIT)
}

/// Make the system trust our CA, returning where it landed.
fn trust_ca(
    target: &TrustTarget,
    authority_public: &Path,
    name: &str,
) -> Result<PathBuf> {
    let _ = kind_of_target(target);
    match target {
        TrustTarget::Dir {
            dir,
            extension,
            update_command,
        } => {
            let placed = dir.join(trust_file_name(name, extension));
            copy(authority_public, &placed)?;
            // No arguments of our own: bare `update-ca-trust` already extracts, and the
            // bare form is equally correct for `update-ca-certificates`.
            if let Err(e) = run(update_command, &[]) {
                let _ = fs::remove_file(&placed);
                return Err(e);
            }
            Ok(placed)
        }
        TrustTarget::Keychain { program, keychain } => {
            let cert = authority_public.to_string_lossy().into_owned();
            let key = keychain.to_string_lossy().into_owned();
            run(program, &["add-trusted-cert", "-d", "-r", "trustRoot", "-k", &key, &cert])?;
            Ok(keychain.clone())
        }
    }
}

fn run(
    command: &str,
    args: &[&str],
) -> Result<()> {
    let output = process::Command::new(command)
        .args(args)
        .output()
        .map_err(|e| CertError::io(format!("running {command}"), e))?;
    if !output.status.success() {
        return Err(CertError::Command {
            command: command.to_string(),
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(())
}
