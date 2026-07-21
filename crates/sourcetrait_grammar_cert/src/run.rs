use crate::*;

const DEFAULT_NAME: &str = "grammar";

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
        /// Base directory; the artifacts are written to its `certs` subdir, which must
        /// not already exist.
        base: PathBuf,
    },
    /// Place every artifact. Needs elevation ONLY for the trust anchor.
    Install {
        /// The base passed to `generate`.
        base: PathBuf,
        /// Base for the runtime key material; it lands in this dir's `certs` subdir.
        #[arg(long)]
        secret_base: PathBuf,
        #[arg(long, default_value = DEFAULT_NAME)]
        name: String,
        /// Override the detected trust store with a directory-model anchor dir.
        #[arg(long)]
        anchor_dir: Option<PathBuf>,
        /// User to hand the non-anchor artifacts to. Defaults to `$SUDO_USER`.
        #[arg(long)]
        owner: Option<String>,
        #[arg(long)]
        update_command: Option<String>,
    },
    /// Confirm the anchor took and the key is readable. Run UNPRIVILEGED.
    Verify {
        /// The base passed to `install --secret-base`.
        secret_base: PathBuf,
        #[arg(long, default_value = DEFAULT_NAME)]
        name: String,
        #[arg(long)]
        anchor_dir: Option<PathBuf>,
        #[arg(long)]
        bundle: Option<PathBuf>,
    },
    /// Report which trust-store layout this system presents.
    Store,
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
            anchor_dir.as_deref(),
            owner.as_deref(),
            update_command.as_deref(),
        ),
        Command::Verify {
            secret_base,
            name,
            anchor_dir,
            bundle,
        } => cmd_verify(&secret_base, &name, anchor_dir.as_deref(), bundle.as_deref()),
        Command::Store => cmd_store(),
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
    anchor_dir: Option<&Path>,
    owner: Option<&str>,
    update_command: Option<&str>,
) -> Result<()> {
    let store = crate::store::resolve(anchor_dir, update_command, None)?;
    match store.detected {
        Some(id) => eprintln!("grammar_cert: trust store {id} ({:?})", store.kind()),
        None => eprintln!("grammar_cert: trust store from arguments ({:?})", store.kind()),
    }
    let installed = install(&crate::install::InstallPlan {
        cert_base: base,
        name,
        store: &store,
        secret_base,
        owner,
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
    anchor_dir: Option<&Path>,
    bundle: Option<&Path>,
) -> Result<()> {
    let store = crate::store::resolve(anchor_dir, None, bundle)?;
    let verified = verify(&crate::verify::VerifyPlan {
        secret_base,
        name,
        store: &store,
    })?;
    println!("key readable: {}", verified.key_path.display());
    if !verified.anchor_trusted {
        return Err(CertError::msg(format!(
            "anchor is not trusted in {}",
            verified.trust_source,
        )));
    }
    println!("anchor trusted: {}", verified.trust_source);
    Ok(())
}

/// Report what we detect, so a platform we do not know is a legible answer rather than
/// a confusing failure inside `install`.
fn cmd_store() -> Result<()> {
    match crate::store::detect() {
        Some(store) => {
            println!("{}", store.id());
            println!("{:?}", store.kind());
            Ok(())
        }
        None => Err(CertError::msg("no known trust store found")),
    }
}
