//! The runtime configuration, split format-layer from model-layer.
use crate::*;

/// The embedded base every load merges onto.
const DEFAULTS_CONFIG: &str = include_str!("../defaults/grammar_mcp.toml");

/// The namespace when `--namespace` is not given.
pub(crate) const DEFAULT_NAMESPACE: &str = "default";

/// The namespace `--test` defaults to.
pub(crate) const TEST_NAMESPACE: &str = "test";

/// Lowest port the channel hub may be pinned to.
const MIN_CHANNEL_PORT: u16 = 1024;

/// The file shape.
#[derive(Debug, Clone, Default, ser::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfigToml {
    pub channel: Option<ChannelConfigToml>,
    pub supervisor: Option<SupervisorConfigToml>,
}

/// One `[[remote]]` entry (file layer): a peer to link with, addressed by alias.
/// Role is the presence of `listen` - set means this host binds and waits
/// (listener), unset means it dials `address` (connector). The field names are
/// the operator's and are never renamed for internal use.
#[derive(Debug, Clone, ser::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteToml {
    /// The name remote_channel_open addresses this peer by.
    pub alias: Option<String>,
    /// `ip:port` to bind; present = listener, absent = connector.
    pub listen: Option<String>,
    /// Connector: the peer's `ip:port` to dial (required). Listener: optional -
    /// a bare source IP to require on an inbound connection (no port).
    pub address: Option<String>,
    /// This host's own public key it presents.
    pub self_public_key_file: Option<String>,
    /// The private key paired with `self_public_key_file`.
    pub self_private_key_file: Option<String>,
    /// The peer's known public key to match; `remote_` is implied by the entry.
    pub public_key_file: Option<String>,
}

/// The `.grammar/mcp/remotes.toml` file (file layer): the RemoteChannel peer set,
/// as `[[remote]]` entries. Optional - an absent file means no RemoteChannel.
#[derive(Debug, Clone, Default, ser::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemotesConfigToml {
    #[serde(default)]
    pub remote: Vec<RemoteToml>,
}

/// The `[supervisor]` table: when a resource level is worth one warning.
#[derive(Debug, Clone, Default, ser::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SupervisorConfigToml {
    /// Fraction of ALL cores the host's own CPU must reach.
    pub cpu_warn_fraction: Option<f64>,
    /// Fraction of total system RAM the host's own RSS must reach.
    pub ram_warn_fraction: Option<f64>,
    /// Warn once GPU memory FREE falls below this many MiB.
    pub vram_warn_headroom_mib: Option<u64>,
    /// Fraction of a watched filesystem's capacity.
    pub disk_warn_fraction: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SupervisorConfig {
    pub cpu_warn_fraction: f64,
    pub ram_warn_fraction: f64,
    pub vram_warn_headroom_mib: u64,
    pub disk_warn_fraction: f64,
}

/// The `[channel]` table: the wss port, the cert dir, and the spam policy.
#[derive(Debug, Clone, Default, ser::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChannelConfigToml {
    /// Omit for a kernel-assigned port, which is the default.
    pub port: Option<u16>,
    pub cert_dir: Option<String>,
    /// Windows are whole seconds here and on `config_channel`.
    pub spam_warn_window_secs: Option<u64>,
    pub spam_warn_rate: Option<u32>,
    pub spam_error_window_secs: Option<u64>,
    pub spam_error_rate: Option<u32>,
}

#[derive(Debug, Clone)]
pub(crate) struct Config {
    pub id: String,
    pub namespace: String,
    pub work_dir: PathBuf,
    pub deny: DenySet,
    /// Was this host launched with `--test`?
    pub test: bool,
    pub channel: ChannelConfig,
    pub supervisor: SupervisorConfig,
    /// Configured remote peers (from `.grammar/mcp/remotes.toml`).
    pub remotes: RemotesConfig,
    /// The process's cwd at startup, captured before any interact() `cd` can
    /// move it - the root `.grammar/mcp/remotes.toml` is resolved against it.
    /// Held for future cwd-relative reads; not read again after the first load.
    #[allow(dead_code)]
    pub startup_cwd: PathBuf,
}

/// A resolved remote peer (model layer): its role plus its known-public-key mTLS
/// material. Field names mirror the toml.
#[derive(Debug, Clone)]
pub(crate) struct RemoteConfig {
    pub alias: String,
    pub role: RemoteRole,
    /// This host's own public key it presents.
    pub self_public_key_file: PathBuf,
    /// The private key paired with `self_public_key_file`.
    pub self_private_key_file: PathBuf,
    /// The peer's known public key, matched byte-for-byte.
    pub peer_public_key_file: PathBuf,
}

/// The resolved RemoteChannel peer set (model layer), by alias.
#[derive(Debug, Clone, Default)]
pub(crate) struct RemotesConfig {
    pub by_alias: HashMap<String, RemoteConfig>,
}

/// A remote entry's role, decided by the presence of `listen`.
#[derive(Debug, Clone)]
pub(crate) enum RemoteRole {
    /// Dial the peer at this address.
    Connector { addr: std::net::SocketAddr },
    /// Bind here and wait; if `allow` is set, require that inbound source IP.
    Listener {
        bind: std::net::SocketAddr,
        allow: Option<std::net::IpAddr>,
    },
}

/// The channel hub's operating parameters.
#[derive(Debug, Clone)]
pub(crate) struct ChannelConfig {
    /// None means kernel-assigned, which is the default.
    pub port: Option<u16>,
    /// Fully expanded at load, like every other path.
    pub cert_dir: PathBuf,
    /// The starting spam policy.
    pub spam: SpamThresholds,
}

/// How much one origin may emit before it is warned, then stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpamThresholds {
    pub warn_window: tk::TkDuration,
    pub warn_rate: u32,
    pub error_window: tk::TkDuration,
    pub error_rate: u32,
}

impl SpamThresholds {
    /// The longer of the two windows.
    pub(crate) fn retention(&self) -> tk::TkDuration {
        if self.warn_window > self.error_window {
            self.warn_window
        } else {
            self.error_window
        }
    }
}

impl ChannelConfig {
    /// The leaf the hub presents, and its key.
    pub(crate) fn cert_paths(&self) -> (PathBuf, PathBuf) {
        let name = lib_grammar::consts::GRAMMAR;
        (
            self.cert_dir.join(format!("entity_{name}.pem")),
            self.cert_dir.join(format!("entity_{name}.key.pem")),
        )
    }
}

/// Merge one `[channel]` table over the embedded base, then the code defaults.
fn merged_channel(
    user: Option<ChannelConfigToml>,
    base: Option<ChannelConfigToml>,
) -> Result<ChannelConfig, String> {
    let user = user.unwrap_or_default();
    let base = base.unwrap_or_default();
    let Some(cert_dir) = user.cert_dir.or(base.cert_dir) else {
        return Err("the embedded defaults carry no channel.cert_dir".to_string());
    };
    let port = user.port.or(base.port);
    if let Some(port) = port
        && port < MIN_CHANNEL_PORT
    {
        return Err(format!(
            "channel.port must be {MIN_CHANNEL_PORT}-65535 (below that is privileged); \
             omit it for a kernel-assigned port",
        ));
    }
    let spam = SpamThresholds {
        warn_window: secs_field("spam_warn_window_secs", user.spam_warn_window_secs, base.spam_warn_window_secs)?,
        warn_rate: rate_field("spam_warn_rate", user.spam_warn_rate, base.spam_warn_rate)?,
        error_window: secs_field("spam_error_window_secs", user.spam_error_window_secs, base.spam_error_window_secs)?,
        error_rate: rate_field("spam_error_rate", user.spam_error_rate, base.spam_error_rate)?,
    };
    Ok(ChannelConfig {
        port,
        cert_dir: expand_path(Path::new(&cert_dir))?,
        spam,
    })
}

fn merged_supervisor(
    user: Option<SupervisorConfigToml>,
    base: Option<SupervisorConfigToml>,
) -> Result<SupervisorConfig, String> {
    let user = user.unwrap_or_default();
    let base = base.unwrap_or_default();
    Ok(SupervisorConfig {
        cpu_warn_fraction: fraction_field(
            "cpu_warn_fraction",
            user.cpu_warn_fraction.or(base.cpu_warn_fraction),
        )?,
        ram_warn_fraction: fraction_field(
            "ram_warn_fraction",
            user.ram_warn_fraction.or(base.ram_warn_fraction),
        )?,
        vram_warn_headroom_mib: user
            .vram_warn_headroom_mib
            .or(base.vram_warn_headroom_mib)
            .ok_or_else(|| "the embedded defaults carry no supervisor.vram_warn_headroom_mib".to_string())?,
        disk_warn_fraction: fraction_field(
            "disk_warn_fraction",
            user.disk_warn_fraction.or(base.disk_warn_fraction),
        )?,
    })
}

/// Load `.grammar/mcp/remotes.toml` under the startup cwd, if present. Absent =
/// no RemoteChannel (an empty peer set).
fn load_remotes(startup_cwd: &std::path::Path) -> Result<RemotesConfig, String> {
    let path = startup_cwd
        .join(".grammar")
        .join("mcp")
        .join("remotes.toml");
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(RemotesConfig::default()),
        Err(e) => return Err(format!("read {}: {e}", path.display())),
    };
    let file: RemotesConfigToml =
        toml::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
    RemotesConfig::try_from(file)
}

impl TryFrom<RemoteToml> for RemoteConfig {
    type Error = String;

    /// Resolve one `[[remote]]` entry into a peer. Role is the presence of
    /// `listen`; the three key paths are required and expanded at load.
    fn try_from(entry: RemoteToml) -> Result<Self, String> {
        let alias = entry
            .alias
            .ok_or_else(|| "a [[remote]] entry is missing `alias`".to_string())?;
        let missing = |field: &str| format!("[[remote]] `{alias}` is missing `{field}`");
        let self_public = entry
            .self_public_key_file
            .ok_or_else(|| missing("self_public_key_file"))?;
        let self_private = entry
            .self_private_key_file
            .ok_or_else(|| missing("self_private_key_file"))?;
        let peer_public = entry
            .public_key_file
            .ok_or_else(|| missing("public_key_file"))?;
        let role = match entry.listen {
            Some(listen) => {
                let bind = listen.parse::<std::net::SocketAddr>().map_err(|e| {
                    format!("[[remote]] `{alias}` listen `{listen}` is not ip:port: {e}")
                })?;
                let allow = match entry.address {
                    Some(addr) => Some(addr.parse::<std::net::IpAddr>().map_err(|e| {
                        format!(
                            "[[remote]] `{alias}` address `{addr}` is not a bare IP (a listener's `address` is a source-IP filter, no port): {e}"
                        )
                    })?),
                    None => None,
                };
                RemoteRole::Listener { bind, allow }
            }
            None => {
                let addr = entry.address.ok_or_else(|| {
                    format!(
                        "[[remote]] `{alias}` has no `listen`, so it is a connector and needs `address` to dial",
                    )
                })?;
                let addr = addr.parse::<std::net::SocketAddr>().map_err(|e| {
                    format!("[[remote]] `{alias}` address `{addr}` is not ip:port: {e}")
                })?;
                RemoteRole::Connector { addr }
            }
        };
        Ok(RemoteConfig {
            alias,
            role,
            self_public_key_file: expand_path(Path::new(&self_public))?,
            self_private_key_file: expand_path(Path::new(&self_private))?,
            peer_public_key_file: expand_path(Path::new(&peer_public))?,
        })
    }
}

impl TryFrom<RemotesConfigToml> for RemotesConfig {
    type Error = String;

    /// Resolve every `[[remote]]` entry, keyed by alias; a duplicate alias is a
    /// load error. There are no embedded defaults (deployment-specific).
    fn try_from(file: RemotesConfigToml) -> Result<Self, String> {
        let mut by_alias = HashMap::new();
        for entry in file.remote {
            let resolved = RemoteConfig::try_from(entry)?;
            if by_alias.contains_key(&resolved.alias) {
                return Err(format!("duplicate [[remote]] alias `{}`", resolved.alias));
            }
            by_alias.insert(resolved.alias.clone(), resolved);
        }
        Ok(RemotesConfig { by_alias })
    }
}

/// A fraction of total capacity; outside (0, 1] it would warn always or never.
pub(crate) fn fraction_field(
    name: &str,
    value: Option<f64>,
) -> Result<f64, String> {
    match value {
        None => Err(format!("the embedded defaults carry no supervisor.{name}")),
        Some(f) if f <= 0.0 || f > 1.0 => {
            Err(format!("supervisor.{name} must be greater than 0 and at most 1"))
        }
        Some(f) => Ok(f),
    }
}

/// A window, in whole seconds.
fn secs_field(
    name: &str,
    user: Option<u64>,
    base: Option<u64>,
) -> Result<tk::TkDuration, String> {
    match user.or(base) {
        Some(0) | None => Err(format!("channel.{name} must be a positive number of seconds")),
        Some(secs) => Ok(tk::TkDuration::from_secs(secs)),
    }
}

/// A rate.
fn rate_field(
    name: &str,
    user: Option<u32>,
    base: Option<u32>,
) -> Result<u32, String> {
    match user.or(base) {
        Some(0) | None => Err(format!("channel.{name} must be greater than zero")),
        Some(rate) => Ok(rate),
    }
}

impl Config {
    /// Build the runtime config from the file layer plus the arguments.
    pub(crate) fn from_toml(
        user: ConfigToml,
        id: String,
        namespace: String,
        work_dir: PathBuf,
        deny: DenySet,
        test: bool,
    ) -> Result<Self, String> {
        let base: ConfigToml = toml::from_str(DEFAULTS_CONFIG)
            .map_err(|e| format!("the embedded defaults do not parse: {e}"))?;
        let channel = merged_channel(user.channel, base.channel)?;
        let supervisor = merged_supervisor(user.supervisor, base.supervisor)?;
        let startup_cwd =
            std::env::current_dir().map_err(|e| format!("cannot read the startup cwd: {e}"))?;
        let remotes = load_remotes(&startup_cwd)?;
        Ok(Self {
            id,
            namespace,
            work_dir,
            deny,
            test,
            channel,
            supervisor,
            remotes,
            startup_cwd,
        })
    }

    /// Read a user file; an absent or malformed `--config` is an error.
    pub(crate) fn read_toml(path: &std::path::Path) -> Result<ConfigToml, String> {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("read config {}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("parse config {}: {e}", path.display()))
    }
}

impl Default for Config {
    fn default() -> Self {
        let id = default_id();
        let work_dir = default_work_dir(&id);
        Self::from_toml(
            ConfigToml::default(),
            id,
            DEFAULT_NAMESPACE.to_string(),
            work_dir,
            DenySet::default(),
            false,
        )
        .expect("embedded defaults parse")
    }
}

/// The invoking user's name, for the zero-config human case.
pub(crate) fn default_id() -> String {
    std::env::var("USER").unwrap_or_else(|_| "default".to_string())
}

pub(crate) fn default_work_dir(id: &str) -> PathBuf {
    BASE_DIRS.home_dir().join("proj").join("equip").join(id)
}

/// Expand `~` and `$VAR` references so config files stay portable.
pub(crate) fn expand_path(raw: &Path) -> Result<PathBuf, String> {
    let lossy = raw.to_string_lossy();
    if !lossy.contains(['$', '~']) {
        return Ok(raw.to_path_buf());
    }
    shellexpand::full_with_context(lossy.as_ref(), home_dir, |var: &str| {
        if let Ok(value) = std::env::var(var) {
            return Ok(Some(value));
        }
        match spec_default(var) {
            Some(default) => Ok(Some(default.to_string_lossy().into_owned())),
            None => Err("environment variable not found".to_string()),
        }
    })
    .map(|expanded| PathBuf::from(expanded.as_ref()))
    .map_err(|e| format!("expand {}: ${}: {}", raw.display(), e.var_name, e.cause))
}

/// `$HOME`, read per call: the tilde source and the spec defaults' base.
fn home_dir() -> Option<String> {
    std::env::var("HOME").ok()
}

/// The XDGX base spec in force: the env value, else its default `xdg`.
fn xdgx_base_spec() -> String {
    std::env::var("XDGX_BASE_SPEC").unwrap_or_else(|_| "xdg".to_string())
}

/// The XDG basedir / SourceTrait XDGX spec default for an unset variable.
pub(crate) fn spec_default(var: &str) -> Option<PathBuf> {
    spec_default_for(var, &xdgx_base_spec())
}

/// The spec default under an explicit base spec; the pure, testable core.
pub(crate) fn spec_default_for(
    var: &str,
    base_spec: &str,
) -> Option<PathBuf> {
    if var == "XDGX_BASE_SPEC" {
        return Some(PathBuf::from(base_spec));
    }
    if var == "XDGX_SHM_DIR" {
        return Some(PathBuf::from("/dev/shm").join(default_id()));
    }
    let dotsys = base_spec == "dotsys";
    let home_relative = match var {
        "XDG_CACHE_HOME" => ".cache",
        "XDG_CONFIG_HOME" => ".config",
        "XDG_DATA_HOME" => ".local/share",
        "XDG_STATE_HOME" => ".local/state",
        // SourceTrait's own concept: a standard user-level place for modern
        // vendors to install to; EXECUTE_HOME is expected on the user's PATH.
        "XDGX_ASSET_HOME" => if dotsys { ".sys/local/share" } else { ".local/share" },
        "XDGX_EXECUTE_HOME" => if dotsys { ".sys/local/bin" } else { ".local/bin" },
        "XDGX_LIBRARY_HOME" => if dotsys { ".sys/local/lib" } else { ".local/lib" },
        "XDGX_PACKAGE_HOME" => if dotsys { ".sys/local/pkg" } else { ".local/pkg" },
        "XDGX_SECRET_DATA_HOME" => {
            if dotsys { ".sys/.xdg/secret/data" } else { ".secret/data" }
        }
        "XDGX_TMP_HOME" => "tmp",
        _ => return None,
    };
    home_dir().map(|home| PathBuf::from(home).join(home_relative))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeniableTool {
    Run,
    Rerun,
    Interact,
    Call,
    Learn,
    New,
    Commit,
    Rig,
    ChannelOpen,
    ChannelVerified,
    ChannelClose,
    ConfigChannel,
    Purviews,
    PurviewConfigure,
    PurviewExtend,
    Purview,
    RemoteChannelOpen,
    RemoteChannelClose,
    RemoteChannels,
}

impl DeniableTool {
    pub(crate) fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "run" => Self::Run,
            "rerun" => Self::Rerun,
            "interact" => Self::Interact,
            "call" => Self::Call,
            "learn" => Self::Learn,
            "new" => Self::New,
            "commit" => Self::Commit,
            "rig" => Self::Rig,
            "channel_open" => Self::ChannelOpen,
            "channel_verified" => Self::ChannelVerified,
            "channel_close" => Self::ChannelClose,
            "config_channel" => Self::ConfigChannel,
            "purviews" => Self::Purviews,
            "purview_configure" => Self::PurviewConfigure,
            "purview_extend" => Self::PurviewExtend,
            "purview" => Self::Purview,
            "remote_channel_open" => Self::RemoteChannelOpen,
            "remote_channel_close" => Self::RemoteChannelClose,
            "remote_channels" => Self::RemoteChannels,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DenySet {
    denied: Vec<DeniableTool>,
}

impl DenySet {
    pub(crate) fn new(denied: Vec<DeniableTool>) -> Self {
        Self { denied }
    }

    pub(crate) fn denies(
        &self,
        tool: DeniableTool,
    ) -> bool {
        self.denied.contains(&tool)
    }
}

pub(crate) static CONFIG: OnceLock<Config> = OnceLock::new();

pub(crate) fn config() -> &'static Config {
    CONFIG.get().expect("CONFIG set at startup")
}
