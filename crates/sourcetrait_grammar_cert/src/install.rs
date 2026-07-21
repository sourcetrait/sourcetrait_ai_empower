use crate::*;

/// Where each artifact ends up. `generate` deliberately knows none of this.
pub(crate) struct InstallPlan<'a> {
    /// The base `generate` was given; its artifacts are in `<base>/certs`.
    pub cert_base: &'a Path,
    /// Filename stem, matching the `name` used at generate time.
    pub name: &'a str,
    /// The resolved trust-store layout: anchor dir, anchor extension, refresh command.
    /// The anchor dir is the one path used verbatim - the store owns its layout, so no
    /// `certs` leaf goes there.
    pub store: &'a crate::store::Resolved,
    /// Base for the runtime key material; the keys land in `<base>/certs`.
    pub secret_base: &'a Path,
    /// User to hand the non-anchor artifacts to. Defaults to `$SUDO_USER`.
    pub owner: Option<&'a str>,
}

pub(crate) struct Installed {
    pub anchor: PathBuf,
    pub secrets: Vec<PathBuf>,
    pub owner: Option<String>,
}

/// Place every artifact, then hand the non-anchor ones back to the invoking user.
///
/// The ownership step is not housekeeping. `install` runs under `sudo`, so anything it
/// creates is root-owned; leave it that way and the unprivileged MCP cannot read the CA
/// key it is supposed to own, which surfaces much later as a channel-open failure long
/// after the install reported success.
pub(crate) fn install(plan: &InstallPlan<'_>) -> Result<Installed> {
    // PREFLIGHT: every check before any mutation, so a bad argument cannot leave the
    // system half-installed.
    let source_dir = crate::generate::certs_dir(plan.cert_base);
    let files = CertFiles::new(&source_dir, plan.name);
    if !files.exist() {
        return Err(CertError::NotACertDir {
            path: source_dir.display().to_string(),
            reason: format!("missing `{}` artifacts", plan.name),
        });
    }

    // The anchor dir must ALREADY EXIST. We never create it: its existence is the
    // evidence that this platform's trust store is where we think it is, and creating
    // it would turn a typo into a silent success - a cert dropped in a directory
    // nothing reads, with the trust never taking and no error to show for it.
    if let crate::store::Placement::AnchorDir { dir, .. } = &plan.store.placement {
        require_existing_dir(dir, "trust anchor dir")?;
    }
    require_existing_dir(plan.secret_base, "secret base")?;

    let secret_dir = crate::generate::certs_dir(plan.secret_base);
    if secret_dir.exists() {
        return Err(CertError::CertsDirExists {
            path: secret_dir.display().to_string(),
        });
    }
    let owner = resolve_owner(plan.owner)?;

    // ACT. Our own key material first: it is the reversible half, and placing the
    // anchor last means a failure here never leaves a trusted CA whose key we did not
    // finish installing.
    fs::create_dir_all(&secret_dir)
        .map_err(|e| CertError::io(format!("creating {}", secret_dir.display()), e))?;
    let mut secrets = Vec::new();
    // The authority PUBLIC cert rides along too. It is not a secret, but `verify` needs
    // the CA body to look for in the trust store, and carrying it here keeps verify
    // working identically whether the anchor became a file or a keychain entry.
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
        copy(source, &dest)?;
        secrets.push(dest);
    }
    if let Some((_, uid, gid)) = &owner {
        chown_to(&secret_dir, *uid, *gid)?;
        for path in &secrets {
            chown_to(path, *uid, *gid)?;
        }
    }

    let anchor = place_anchor(&plan.store.placement, &files.authority_public, plan.name)?;

    Ok(Installed {
        anchor,
        secrets,
        owner: owner.map(|(user, _, _)| user),
    })
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

/// `$SUDO_USER` when the caller did not name one. Absent (a non-sudo run) means there
/// is nobody to hand ownership to and the files already belong to the right user, so
/// the step is skipped rather than guessed at.
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

/// Copy, creating NOTHING. An earlier cut created the destination's parent, which is
/// how a typo'd `--anchor-dir` would have become a freshly-minted system directory
/// holding a cert nothing reads. Directories are either ours to make deliberately
/// (the `certs` leaf) or must already exist.
fn copy(
    from: &Path,
    to: &Path,
) -> Result<()> {
    fs::copy(from, to).map_err(|e| {
        CertError::io(format!("copying {} to {}", from.display(), to.display()), e)
    })?;
    Ok(())
}

/// Hand the CA certificate to whichever trust model this platform uses, returning where
/// it landed (a file path, or the keychain it was ingested into).
fn place_anchor(
    placement: &crate::store::Placement,
    authority_public: &Path,
    name: &str,
) -> Result<PathBuf> {
    match placement {
        crate::store::Placement::AnchorDir {
            dir,
            extension,
            update_command,
            ..
        } => {
            let anchor = dir.join(format!("{name}.{extension}"));
            copy(authority_public, &anchor)?;
            // NO arguments of our own. An earlier cut passed `extract`, which couples
            // us to one distribution's subcommand vocabulary even while the command
            // itself is a flag; bare `update-ca-trust` already extracts, and the bare
            // form is equally correct for `update-ca-certificates`.
            run(update_command, &[])?;
            Ok(anchor)
        }
        crate::store::Placement::Keychain { program, keychain } => {
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
