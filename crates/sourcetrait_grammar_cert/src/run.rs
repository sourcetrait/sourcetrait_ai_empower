use crate::*;

const DEFAULT_NAME: &str = "grammar";

/// Default for `--secret-data`. sudo's env_reset strips it, so an elevated run needs
/// `sudo -E` or the explicit flag.
const SECRET_DATA_ENV: &str = "XDGX_SECRET_DATA_HOME";

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
        /// Base directory; the artifacts are written to its `certs` subdir, which must
        /// not already exist.
        base: PathBuf,
        /// Parameters: subjects, SANs, validity. Carries no paths. Defaults to the
        /// built-in config.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Place the key material and make the system trust the CA.
    Install {
        /// The base passed to `generate`.
        base: PathBuf,
        /// The secret DATA home; key material lands in its
        /// `sourcetrait/grammar/certs` subdir. Defaults to $XDGX_SECRET_DATA_HOME.
        #[arg(long)]
        secret_data: Option<PathBuf>,
        #[arg(long, default_value = DEFAULT_NAME)]
        name: String,
        /// Override the detected trust store with a directory-model trust dir.
        #[arg(long)]
        trust_dir: Option<PathBuf>,
        /// User to hand the key material to. Defaults to `$SUDO_USER`.
        #[arg(long)]
        owner: Option<String>,
        #[arg(long)]
        update_command: Option<String>,
    },
    /// Confirm the CA is trusted and the keys are readable. Run UNPRIVILEGED.
    Verify {
        /// The path passed to `install --secret-data`.
        /// Defaults to $XDGX_SECRET_DATA_HOME.
        secret_data: Option<PathBuf>,
        #[arg(long, default_value = DEFAULT_NAME)]
        name: String,
        /// The CA cert to look for. Defaults to the installed copy.
        #[arg(long)]
        ca_cert: Option<PathBuf>,
        #[arg(long)]
        bundle: Option<PathBuf>,
    },
    /// Report which trust-store layout this system presents.
    Store,
}

pub fn run() -> Result<()> {
    match <Cli as clap::Parser>::parse().command {
        Command::Generate { base, config } => cmd_generate(config.as_deref(), &base),
        Command::Install {
            base,
            secret_data,
            name,
            trust_dir,
            owner,
            update_command,
        } => cmd_install(
            &base,
            &resolve_secret_data(secret_data)?,
            &name,
            trust_dir.as_deref(),
            owner.as_deref(),
            update_command.as_deref(),
        ),
        Command::Verify {
            secret_data,
            name,
            ca_cert,
            bundle,
        } => cmd_verify(
            &resolve_secret_data(secret_data)?,
            &name,
            ca_cert.as_deref(),
            bundle.as_deref(),
        ),
        Command::Store => cmd_store(),
    }
}

fn resolve_secret_data(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    match std::env::var(SECRET_DATA_ENV) {
        Ok(v) if !v.is_empty() => Ok(PathBuf::from(v)),
        _ => Err(CertError::msg(format!(
            "pass --secret-data or set ${SECRET_DATA_ENV}"
        ))),
    }
}

/// The built-in parameters, used when `--config` is not given. Embedded so the common
/// case needs no file at all.
const BUILTIN_CONFIG: &str = include_str!("../assets/certgen.toml");

fn cmd_generate(
    config_path: Option<&Path>,
    base: &Path,
) -> Result<()> {
    let config = match config_path {
        Some(path) => CertGenConfig::load(path)?,
        None => CertGenConfig::parse(BUILTIN_CONFIG, Path::new("<built-in>"))?,
    };
    let files = generate(&config, base)?;
    for path in files.all() {
        println!("{}", path.display());
    }
    Ok(())
}

fn cmd_install(
    base: &Path,
    secret_data: &Path,
    name: &str,
    trust_dir: Option<&Path>,
    owner: Option<&str>,
    update_command: Option<&str>,
) -> Result<()> {
    let (target, detected) = crate::store::resolve_target(trust_dir, update_command)?;
    match detected {
        Some(id) => eprintln!("grammar_cert: trust store {id}"),
        None => eprintln!("grammar_cert: trust store from arguments"),
    }
    let installed = install(&crate::install::InstallPlan {
        cert_base: base,
        name,
        target: &target,
        secret_data,
        owner,
    })?;
    println!("{}", installed.trusted_at.display());
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
    secret_data: &Path,
    name: &str,
    ca_cert: Option<&Path>,
    bundle: Option<&Path>,
) -> Result<()> {
    let source = crate::store::resolve_source(bundle)?;
    let verified = verify(&crate::verify::VerifyPlan {
        secret_data,
        name,
        source: &source,
        ca_cert,
    })?;
    for path in &verified.keys_read {
        println!("key readable: {}", path.display());
    }
    if !verified.ca_trusted {
        return Err(CertError::msg(format!(
            "CA is not trusted in {}",
            verified.trust_source,
        )));
    }
    println!("ca trusted: {}", verified.trust_source);
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
