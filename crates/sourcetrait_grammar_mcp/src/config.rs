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
    pub remote: Option<RemoteToml>,
}

/// One `[remote.<alias>]` table: a peer to link with. All fields required.
#[derive(Debug, Clone, ser::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteAliasToml {
    /// The peer's socket address, `ip:port`.
    pub address: Option<String>,
    /// This host's own entity leaf presented to the peer (public).
    pub self_public_key_file: Option<String>,
    /// The private key paired with `self_public_key_file`.
    pub self_private_key_file: Option<String>,
    /// The peer's entity leaf to pin (public).
    pub remote_public_key_file: Option<String>,
}

/// The `[remote]` table: the acceptor's own listen config plus the peer aliases.
#[derive(Debug, Clone, Default, ser::Deserialize)]
pub(crate) struct RemoteToml {
    /// `ip:port` the acceptor binds; absent = initiator-only (no listener).
    pub listen: Option<String>,
    /// This host's own entity leaf the acceptor presents (public).
    pub self_public_key_file: Option<String>,
    /// The private key paired with `self_public_key_file`.
    pub self_private_key_file: Option<String>,
    /// The peer aliases, `[remote.<alias>]`.
    #[serde(flatten)]
    pub peers: HashMap<String, RemoteAliasToml>,
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
    /// Configured remote peers, by alias.
    pub remote: HashMap<String, RemoteConfig>,
    /// The acceptor's listen config, when `[remote].listen` is set.
    pub remote_listen: Option<RemoteListen>,
}

/// A resolved `[remote.<alias>]` peer: where it is and its mTLS material.
#[derive(Debug, Clone)]
pub(crate) struct RemoteConfig {
    pub addr: std::net::SocketAddr,
    /// This host's own leaf, presented for client auth.
    pub self_cert_file: PathBuf,
    /// The private key paired with `self_cert_file`.
    pub self_key_file: PathBuf,
    /// The peer's leaf, pinned byte-for-byte.
    pub remote_pin_file: PathBuf,
}

/// The resolved `[remote]` acceptor config: its listen address and identity.
#[derive(Debug, Clone)]
pub(crate) struct RemoteListen {
    pub addr: std::net::SocketAddr,
    /// This host's own leaf, presented to inbound peers.
    pub self_cert_file: PathBuf,
    /// The private key paired with `self_cert_file`.
    pub self_key_file: PathBuf,
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
        cert_dir: expand_path(&cert_dir)?,
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

/// Resolve the `[remote]` section: the peer aliases plus this host's own
/// acceptor listen config. There are no embedded defaults.
fn merged_remote(
    user: Option<RemoteToml>,
) -> Result<(HashMap<String, RemoteConfig>, Option<RemoteListen>), String> {
    let RemoteToml {
        listen,
        self_public_key_file,
        self_private_key_file,
        peers,
    } = user.unwrap_or_default();

    let mut out = HashMap::new();
    for (alias, cfg) in peers {
        let address = cfg
            .address
            .ok_or_else(|| format!("[remote.{alias}] is missing `address`"))?;
        let addr = address
            .parse::<std::net::SocketAddr>()
            .map_err(|e| format!("[remote.{alias}] address `{address}` is not ip:port: {e}"))?;
        let self_cert = cfg
            .self_public_key_file
            .ok_or_else(|| format!("[remote.{alias}] is missing `self_public_key_file`"))?;
        let self_key = cfg
            .self_private_key_file
            .ok_or_else(|| format!("[remote.{alias}] is missing `self_private_key_file`"))?;
        let remote_pin = cfg
            .remote_public_key_file
            .ok_or_else(|| format!("[remote.{alias}] is missing `remote_public_key_file`"))?;
        out.insert(
            alias,
            RemoteConfig {
                addr,
                self_cert_file: expand_path(&self_cert)?,
                self_key_file: expand_path(&self_key)?,
                remote_pin_file: expand_path(&remote_pin)?,
            },
        );
    }

    let remote_listen = match (listen, self_public_key_file, self_private_key_file) {
        (None, None, None) => None,
        (Some(addr), Some(cert), Some(key)) => {
            let parsed = addr.parse::<std::net::SocketAddr>().map_err(|e| {
                format!("[remote] listen `{addr}` is not ip:port: {e}")
            })?;
            Some(RemoteListen {
                addr: parsed,
                self_cert_file: expand_path(&cert)?,
                self_key_file: expand_path(&key)?,
            })
        }
        _ => {
            return Err("[remote] listen, self_public_key_file, and self_private_key_file \
                 must all be set together (the acceptor's own listen address and identity)"
                .to_string());
        }
    };

    Ok((out, remote_listen))
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
        let (remote, remote_listen) = merged_remote(user.remote)?;
        Ok(Self {
            id,
            namespace,
            work_dir,
            deny,
            test,
            channel,
            supervisor,
            remote,
            remote_listen,
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

/// A `$VAR` value, honoring the XDG basedir spec fallbacks.
fn var_or_xdg(name: &str) -> Result<String, String> {
    if let Ok(value) = std::env::var(name)
        && !value.is_empty()
    {
        return Ok(value);
    }
    let home = || BASE_DIRS.home_dir().to_string_lossy().into_owned();
    Ok(match name {
        "XDG_DATA_HOME" => format!("{}/.local/share", home()),
        "XDG_CONFIG_HOME" => format!("{}/.config", home()),
        "XDG_STATE_HOME" => format!("{}/.local/state", home()),
        "XDG_CACHE_HOME" => format!("{}/.cache", home()),
        _ => return Err(format!("environment variable ${name} is not set")),
    })
}

/// Expand a leading `~` or `$VAR` segment so config files stay portable.
pub(crate) fn expand_path(raw: &str) -> Result<PathBuf, String> {
    if raw == "~" {
        return Ok(BASE_DIRS.home_dir().to_path_buf());
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        return Ok(BASE_DIRS.home_dir().join(rest));
    }
    if let Some(rest) = raw.strip_prefix('$') {
        let (name, tail) = match rest.find('/') {
            Some(split) => rest.split_at(split),
            None => (rest, ""),
        };
        return Ok(PathBuf::from(format!("{}{tail}", var_or_xdg(name)?)));
    }
    Ok(PathBuf::from(raw))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeniableTool {
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
