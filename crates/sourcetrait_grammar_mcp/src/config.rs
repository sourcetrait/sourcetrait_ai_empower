//! The runtime configuration, split format-layer from model-layer.
//!
//! `*Toml` types are the FILE shape - every field optional, paths as portable strings,
//! unknown keys rejected. `Config` and its sub-items are the format-free runtime shape
//! with concrete types. Sub-items get the same pair, so a future format adds a shell
//! without touching the model.
//!
//! THE FILE CARRIES ONLY WHAT IS NOT ALREADY AN ARGUMENT. The store coordinate (`id`,
//! `namespace`), the agent work dir and the deny list stay ARGUMENTS: they were
//! arguments before this file existed, and they identify or gate the invocation itself.
//! A file-settable coordinate would let the store silently diverge from the `.mcp.json`
//! entry the agent believes it is talking to, and would reintroduce the sticky default
//! coordinate already ruled out. So the two surfaces are DISJOINT - there is no
//! precedence question between them - and `deny_unknown_fields` turns an attempt to set
//! one from the file into a loud error rather than a silent no-op.
use crate::*;

/// The embedded base every load merges onto.
const DEFAULTS_CONFIG: &str = include_str!("../defaults/grammar_mcp.toml");

/// The store namespace when `--namespace` is not given.
pub(crate) const DEFAULT_NAMESPACE: &str = "default";

/// Lowest port the channel hub may be pinned to. Anything below is privileged and the
/// host is unprivileged by design.
const MIN_CHANNEL_PORT: u16 = 1024;

/// The file shape.
#[derive(Debug, Clone, Default, ser::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfigToml {
    pub channel: Option<ChannelConfigToml>,
    pub supervisor: Option<SupervisorConfigToml>,
}

/// The `[supervisor]` table: when a resource level is worth ONE warning to the agent.
///
/// Expressed against the machine's TOTAL capacity rather than as absolute figures, so
/// the same defaults mean the same thing on a different box and do not silently age.
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

/// The `[channel]` table: the wss port and the cert directory, and nothing else.
///
/// Everything else about the channel is fixed by design rather than configured. It is
/// a loopback socket serving a single host, so there is no address to choose and no
/// peer policy to express - the bind is 127.0.0.1 by construction, which is what makes
/// "only localhost gets in" a property of the socket rather than a rule to enforce.
#[derive(Debug, Clone, Default, ser::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChannelConfigToml {
    /// Omit for a kernel-assigned port, which is the default. When present it must be
    /// an unprivileged port (1024-65535); 0 is rejected rather than treated as "any".
    pub port: Option<u16>,
    pub cert_dir: Option<String>,
    /// Windows are INTEGER SECONDS on this surface. MCP args cross as JSON, where a nu
    /// `duration` cannot be represented, and TOML has no duration type either - so the
    /// `10s` form lives in the model and the docs, never on a wire or in a file.
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
    pub channel: ChannelConfig,
    pub supervisor: SupervisorConfig,
}

/// The channel hub's operating parameters. The hub reads these rather than the
/// environment directly, so the cert location is configurable without a rebuild.
#[derive(Debug, Clone)]
pub(crate) struct ChannelConfig {
    /// None = kernel-assigned, which is the default. The agent learns the real endpoint
    /// from `channel_open`'s return, so an unpinned port is the sane default and a
    /// pinned one is the exception. Modelled as an Option rather than a 0 sentinel
    /// because 0 is not a port.
    pub port: Option<u16>,
    /// Fully expanded at load, like every other path out of config or arguments.
    pub cert_dir: PathBuf,
    /// The starting spam policy. Runtime changes go through `config_channel`, which
    /// mutates the channel's live copy rather than this - CONFIG is set once.
    pub spam: SpamThresholds,
}

/// How much a single origin may emit before it is warned, and before it is stopped.
///
/// Two INDEPENDENT (window, rate) pairs so warn and error can measure different things -
/// a short window catches a burst, a longer one catches sustained misbehaviour.
///
/// THESE VALUES ARE INITIAL. There is no basis for them beyond reasoning; only production
/// traffic will say what a normal producer actually does. The gap between legitimate and
/// runaway is enormous rather than marginal - a state lane emits single digits per burst,
/// a loop emits thousands per second - so the numbers barely affect detection and mostly
/// decide how often a well-behaved fast command gets flagged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpamThresholds {
    pub warn_window: tk::TkDuration,
    pub warn_rate: u32,
    pub error_window: tk::TkDuration,
    pub error_rate: u32,
}

impl SpamThresholds {
    /// The longer of the two windows - how far back the counter has to remember.
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
    // Below 1024 is privileged: the host runs unprivileged, so such a bind could only
    // ever fail. Rejecting it at load turns a confusing permission error at
    // channel_open into a legible config error at startup.
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

/// A fraction of total capacity. Outside (0, 1] it would either warn always or never.
fn fraction_field(
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

/// A window, in whole seconds. Zero would mean "no window", which is not a rate at all.
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

/// A rate. Zero would forbid the first send outright rather than police a rate.
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
    /// Build the runtime config: the argument-owned values arrive already resolved, the
    /// file contributes only what it owns.
    ///
    /// Deliberately not `TryFrom`: the model carries fields the format layer does not,
    /// and a constructor that names them keeps that asymmetry visible rather than
    /// hiding it behind a conversion.
    pub(crate) fn from_toml(
        user: ConfigToml,
        id: String,
        namespace: String,
        work_dir: PathBuf,
        deny: DenySet,
    ) -> Result<Self, String> {
        let base: ConfigToml = toml::from_str(DEFAULTS_CONFIG)
            .map_err(|e| format!("the embedded defaults do not parse: {e}"))?;
        Ok(Self {
            id,
            namespace,
            work_dir,
            deny,
            channel: merged_channel(user.channel, base.channel)?,
            supervisor: merged_supervisor(user.supervisor, base.supervisor)?,
        })
    }

    /// Read a user file. An explicit `--config` that is absent or malformed is an
    /// error - a typo'd path must fail rather than silently serve the defaults.
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
        )
        .expect("embedded defaults parse")
    }
}

/// The invoking user's name, for the zero-config human case. Harness `.mcp.json`
/// entries always pass `--id` explicitly.
pub(crate) fn default_id() -> String {
    std::env::var("USER").unwrap_or_else(|_| "default".to_string())
}

pub(crate) fn default_work_dir(id: &str) -> PathBuf {
    BASE_DIRS.home_dir().join("proj").join("equip").join(id)
}

/// A `$VAR` value, honoring the XDG basedir spec fallbacks; anything else must be set.
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

/// Expand a leading `~` (home) or `$VAR` segment so config files stay portable; any
/// other path passes through literally.
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
    Library,
    ChannelOpen,
    ChannelVerified,
    ChannelClose,
    ConfigChannel,
    PurviewList,
    PurviewConfigure,
    PurviewExtend,
    PurviewReset,
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
            "library" => Self::Library,
            "channel_open" => Self::ChannelOpen,
            "channel_verified" => Self::ChannelVerified,
            "channel_close" => Self::ChannelClose,
            "config_channel" => Self::ConfigChannel,
            "purview_list" => Self::PurviewList,
            "purview_configure" => Self::PurviewConfigure,
            "purview_extend" => Self::PurviewExtend,
            "purview_reset" => Self::PurviewReset,
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
