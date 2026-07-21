use crate::*;

use crate::store::{TrustTarget, kind_of_target};

/// Where the secret data home came from, which decides whether we may create it.
///
/// The env default is OUR namespace - a sourcetrait extension nothing else ships - so
/// nobody else will ever make it and creating it correctly is our job. An explicitly
/// passed path is the CALLER asserting a location, and a typo there must fail loudly
/// rather than conjure a deep tree, exactly as for the trust dir.
pub(crate) enum SecretData<'a> {
    FromEnv(&'a Path),
    Explicit(&'a Path),
}

impl<'a> SecretData<'a> {
    pub(crate) fn path(&self) -> &'a Path {
        match self {
            Self::FromEnv(p) | Self::Explicit(p) => p,
        }
    }
}

pub(crate) struct InstallPlan<'a> {
    /// The dir `generate` was given; its artifacts are in `<dir>/certs`.
    pub cert_dir: &'a Path,
    pub name: &'a str,
    pub target: &'a TrustTarget,
    /// The secret DATA home; key material lands in `<it>/sourcetrait/grammar/certs`.
    pub secret_data: SecretData<'a>,
}

pub(crate) struct Installed {
    /// Where the CA ended up: a file, or the keychain that ingested it.
    pub trusted_at: PathBuf,
    pub secrets: Vec<PathBuf>,
}

/// Place the key material, then make the system trust the CA.
///
/// Runs UNPRIVILEGED, elevating only the few commands that touch the system trust store
/// (`run_privileged`). That is what removes the ownership problem rather than managing
/// it: the key material is created by the invoking user in the first place, so there is
/// nothing root-owned to hand back.
pub(crate) fn install(plan: &InstallPlan<'_>) -> Result<Installed> {
    // PREFLIGHT: every check before any mutation.
    let source_dir = crate::generate::certs_dir(plan.cert_dir);
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

    // For an EXPLICIT secret data home the caller is asserting a location, so its
    // PARENT must already exist - that is what makes a typo fail instead of conjuring a
    // tree. We then create the dir itself and our subdirs beneath it. The env default
    // is ours outright (see SecretData).
    if let SecretData::Explicit(path) = &plan.secret_data {
        let parent = path
            .parent()
            .ok_or_else(|| CertError::msg("--secret-data has no parent directory"))?;
        require_existing_dir(parent, "secret data parent")?;
    }

    let secret_dir = crate::generate::secret_certs_dir(plan.secret_data.path());
    if secret_dir.exists() {
        return Err(CertError::CertsDirExists {
            path: secret_dir.display().to_string(),
        });
    }
    // ACT. Key material first - it is the half we can roll back.
    let created = create_private_chain(&secret_dir)?;
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
        // Everything inside the secret tree goes go-rwx, public certs included. They
        // are not secret in themselves, but a 0644 file sitting beside a CA key is an
        // inconsistency waiting to be copied by the next person who adds a file here.
        if let Err(e) = copy(source, &dest).and_then(|()| restrict_file(&dest)) {
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
    })
}

/// Create every missing level down to `target` at 0700.
///
/// Walks up to the first ancestor that exists, so this covers the secret data home
/// ITSELF as well as our subdirs under it - it is our own non-standard path, so nothing
/// else will have made it.
///
/// Each level is created and chmod'd explicitly rather than via `create_dir_all`, which
/// takes its mode from the umask: under root's usual 022 every level came out 0755, a
/// world-traversable directory around a CA key. Returns what we created, newest last,
/// for rollback.
fn create_private_chain(target: &Path) -> Result<Vec<PathBuf>> {
    use std::os::unix::fs::PermissionsExt;

    let mut missing = Vec::new();
    let mut cursor = Some(target);
    while let Some(path) = cursor {
        if path.exists() {
            break;
        }
        missing.push(path.to_path_buf());
        cursor = path.parent();
    }
    missing.reverse();

    let mut created = Vec::new();
    for path in missing {
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
    }
    Ok(created)
}

/// go-rwx on a file we placed in the secret tree.
fn restrict_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|e| CertError::io(format!("stat {}", path.display()), e))?
        .permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms)
        .map_err(|e| CertError::io(format!("chmod 600 {}", path.display()), e))
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
            let src = authority_public.to_string_lossy().into_owned();
            let dst = placed.to_string_lossy().into_owned();
            run_privileged("cp", &[&src, &dst])?;
            // No arguments of our own: bare `update-ca-trust` already extracts, and the
            // bare form is equally correct for `update-ca-certificates`.
            if let Err(e) = run_privileged(update_command, &[]) {
                let _ = run_privileged("rm", &["-f", &dst]);
                return Err(e);
            }
            Ok(placed)
        }
        TrustTarget::Keychain { program, keychain } => {
            let cert = authority_public.to_string_lossy().into_owned();
            let key = keychain.to_string_lossy().into_owned();
            run_privileged(
                program,
                &["add-trusted-cert", "-d", "-r", "trustRoot", "-k", &key, &cert],
            )?;
            Ok(keychain.clone())
        }
    }
}

/// Run a command that needs root, elevating only if we are not already root.
///
/// `install` itself runs UNPRIVILEGED and elevates just these few commands. Two reasons.
/// `sudo grammar_cert` cannot resolve at all - sudo's `secure_path` does not include our
/// install home - whereas `cp`, `update-ca-trust` and `security` are all on it. And
/// running unprivileged means the key material is created as the invoking user, so
/// there is no root-created file to hand back and no chown step to get wrong.
///
/// sudo is handed an ARGV, never a shell string: no quoting, no word splitting, nothing
/// a path with a space or a quote can turn into a second command.
///
/// Stdio is INHERITED rather than captured, or sudo's password prompt would be
/// swallowed and the call would appear to hang.
fn run_privileged(
    command: &str,
    args: &[&str],
) -> Result<()> {
    if nix::unistd::Uid::effective().is_root() {
        return run(command, args);
    }
    let mut argv = vec![command];
    argv.extend_from_slice(args);
    let status = process::Command::new("sudo")
        .args(&argv)
        .status()
        .map_err(|e| CertError::io("running sudo".to_string(), e))?;
    if !status.success() {
        return Err(CertError::Command {
            command: format!("sudo {command}"),
            status: status.to_string(),
            stderr: String::new(),
        });
    }
    Ok(())
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
