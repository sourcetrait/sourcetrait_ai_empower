use crate::*;

const DEFAULT_NAME: &str = "grammar";
const DEFAULT_ANCHOR_DIR: &str = "/etc/pki/ca-trust/source/anchors";
const DEFAULT_BUNDLE: &str = "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem";
const DEFAULT_UPDATE_COMMAND: &str = "update-ca-trust";

#[derive(clap::Parser)]
#[command(
    name = "grammar_cert",
    version,
    about = "Local TLS certificate authority for the grammar platform"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Mint a local CA and the leaf it signs. Unprivileged.
    Generate {
        /// Parameters: subjects, SANs, validity. Carries no paths.
        config: PathBuf,
        /// Base directory; the artifacts are written to its `certs` subdir, which
        /// must not already exist.
        base: PathBuf,
    },
    /// Place every artifact. Needs root ONLY for the trust anchor.
    Install {
        /// The base passed to `generate`.
        base: PathBuf,
        /// Base for the runtime key material; the keys land in its `certs` subdir.
        #[arg(long)]
        secret_base: PathBuf,
        #[arg(long, default_value = DEFAULT_NAME)]
        name: String,
        #[arg(long, default_value = DEFAULT_ANCHOR_DIR)]
        anchor_dir: PathBuf,
        /// User to hand the non-anchor artifacts to. Defaults to `$SUDO_USER`.
        #[arg(long)]
        owner: Option<String>,
        #[arg(long, default_value = DEFAULT_UPDATE_COMMAND)]
        update_command: String,
    },
    /// Confirm the anchor took and the key is readable. Run UNPRIVILEGED.
    Verify {
        /// The base passed to `install --secret-base`.
        secret_base: PathBuf,
        #[arg(long, default_value = DEFAULT_NAME)]
        name: String,
        #[arg(long, default_value = DEFAULT_ANCHOR_DIR)]
        anchor_dir: PathBuf,
        #[arg(long, default_value = DEFAULT_BUNDLE)]
        bundle: PathBuf,
    },
}

pub fn run() -> Result<()> {
    match <Cli as clap::Parser>::parse().command {
        Command::Generate { config, base } => cmd_generate(&config, &base),
        Command::Install {
            base,
            secret_base,
            name,
            anchor_dir,
            owner,
            update_command,
        } => cmd_install(
            &base,
            &secret_base,
            &name,
            &anchor_dir,
            owner.as_deref(),
            &update_command,
        ),
        Command::Verify {
            secret_base,
            name,
            anchor_dir,
            bundle,
        } => cmd_verify(&secret_base, &name, &anchor_dir, &bundle),
    }
}

fn cmd_generate(
    config_path: &Path,
    base: &Path,
) -> Result<()> {
    let config = CertGenConfig::load(config_path)?;
    let files = generate(&config, base)?;
    for path in files.all() {
        println!("{}", path.display());
    }
    Ok(())
}

fn cmd_install(
    base: &Path,
    secret_base: &Path,
    name: &str,
    anchor_dir: &Path,
    owner: Option<&str>,
    update_command: &str,
) -> Result<()> {
    let installed = install(&crate::install::InstallPlan {
        cert_base: base,
        name,
        anchor_dir,
        secret_base,
        owner,
        update_command,
    })?;
    println!("{}", installed.anchor.display());
    for path in &installed.secrets {
        println!("{}", path.display());
    }
    match installed.owner {
        Some(user) => eprintln!("grammar_cert: key material owned by {user}"),
        None => eprintln!("grammar_cert: ownership unchanged (no --owner, no $SUDO_USER)"),
    }
    Ok(())
}

fn cmd_verify(
    secret_base: &Path,
    name: &str,
    anchor_dir: &Path,
    bundle: &Path,
) -> Result<()> {
    let verified = verify(&crate::verify::VerifyPlan {
        secret_base,
        name,
        anchor_dir,
        bundle,
    })?;
    println!("key readable: {}", verified.key_path.display());
    println!("anchor present: {}", verified.anchor_path.display());
    if !verified.anchor_in_bundle {
        return Err(CertError::msg(format!(
            "anchor is not in {}; run `sudo grammar_cert install` first",
            bundle.display(),
        )));
    }
    println!("anchor trusted: {}", bundle.display());
    Ok(())
}
