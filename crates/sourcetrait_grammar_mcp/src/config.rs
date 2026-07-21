//! The runtime configuration, split format-layer from model-layer.
//!
//! `*Toml` types are the FILE shape - every field optional, paths as portable strings,
//! unknown keys rejected. `Config` and its sub-items are the format-free runtime shape
//! with concrete types. `TryFrom` bridges them, merging the user's file over the
//! embedded base and then the code defaults, so a future format adds a shell without
//! touching the model.
//!
//! Precedence is CLI > file > embedded base > code default. The CLI arrives AS a
//! `ConfigToml` overlay rather than through a second merge path, which is why the clap
//! options carry no `default_value`: a clap default is indistinguishable from an
//! explicit flag and would silently outrank the file.
use crate::*;

/// The embedded base every load merges onto.
const DEFAULTS_CONFIG: &str = include_str!("../defaults/grammar_mcp.toml");

/// The file shape.
#[derive(Debug, Clone, Default, ser::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfigToml {
    pub id: Option<String>,
    pub namespace: Option<String>,
    pub work_dir: Option<String>,
    pub deny: Option<Vec<String>>,
    pub channel: Option<ChannelConfigToml>,
}

/// The `[channel]` table.
#[derive(Debug, Clone, Default, ser::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChannelConfigToml {
    pub cert_dir: Option<String>,
    pub cert_name: Option<String>,
    pub bind: Option<String>,
    pub verify_timeout_secs: Option<u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct Config {
    pub id: String,
    pub namespace: String,
    pub work_dir: PathBuf,
    pub deny: DenySet,
    pub channel: ChannelConfig,
}

/// The channel hub's operating parameters. The hub reads these rather than the
/// environment directly, so the cert location is configurable without a rebuild.
#[derive(Debug, Clone)]
pub(crate) struct ChannelConfig {
    /// UNEXPANDED on purpose - see `cert_paths`.
    pub cert_dir: String,
    pub cert_name: String,
    pub bind: String,
    pub verify_timeout_secs: u64,
}

impl ChannelConfig {
    /// The leaf the hub presents, and its key.
    ///
    /// Expansion is LAZY rather than done at merge time because channels are OPTIONAL:
    /// the default `cert_dir` names `$XDGX_SECRET_DATA_HOME`, and a box that never
    /// opens a channel need not have it set. Expanding eagerly would make the whole
    /// host fail to start there - and would panic `Config::default()`, which the
    /// in-process test harness builds. A missing variable is a `channel_open` failure,
    /// which is the moment it actually matters.
    pub(crate) fn cert_paths(&self) -> Result<(PathBuf, PathBuf), String> {
        let dir = expand_path(&self.cert_dir)?;
        Ok((
            dir.join(format!("entity_{}.pem", self.cert_name)),
            dir.join(format!("entity_{}.key.pem", self.cert_name)),
        ))
    }
}

/// Merge one `[channel]` table over another, then the code defaults.
fn merged_channel(
    user: Option<ChannelConfigToml>,
    base: Option<ChannelConfigToml>,
) -> Result<ChannelConfig, String> {
    let user = user.unwrap_or_default();
    let base = base.unwrap_or_default();
    let Some(cert_dir) = user.cert_dir.or(base.cert_dir) else {
        return Err("the embedded defaults carry no channel.cert_dir".to_string());
    };
    let Some(cert_name) = user.cert_name.or(base.cert_name) else {
        return Err("the embedded defaults carry no channel.cert_name".to_string());
    };
    let Some(bind) = user.bind.or(base.bind) else {
        return Err("the embedded defaults carry no channel.bind".to_string());
    };
    let verify_timeout_secs = user
        .verify_timeout_secs
        .or(base.verify_timeout_secs)
        .unwrap_or(300);
    if verify_timeout_secs == 0 {
        return Err("channel.verify_timeout_secs must be positive".to_string());
    }
    Ok(ChannelConfig {
        cert_dir,
        cert_name,
        bind,
        verify_timeout_secs,
    })
}

impl TryFrom<ConfigToml> for Config {
    type Error = String;

    fn try_from(user: ConfigToml) -> Result<Self, String> {
        let base: ConfigToml = toml::from_str(DEFAULTS_CONFIG)
            .map_err(|e| format!("the embedded defaults do not parse: {e}"))?;
        let id = user.id.or(base.id).unwrap_or_else(default_id);
        let Some(namespace) = user.namespace.or(base.namespace) else {
            return Err("the embedded defaults carry no namespace".to_string());
        };
        let work_dir = match user.work_dir.or(base.work_dir) {
            Some(raw) => expand_path(&raw)?,
            None => default_work_dir(&id),
        };
        let deny = match user.deny.or(base.deny) {
            Some(names) => {
                let mut tools = Vec::with_capacity(names.len());
                for name in &names {
                    match DeniableTool::from_name(name) {
                        Some(tool) => tools.push(tool),
                        None => return Err(format!("unknown tool `{name}` in deny")),
                    }
                }
                DenySet::new(tools)
            }
            None => DenySet::default(),
        };
        Ok(Self {
            id,
            namespace,
            work_dir,
            deny,
            channel: merged_channel(user.channel, base.channel)?,
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        ConfigToml::default()
            .try_into()
            .expect("embedded defaults parse")
    }
}

impl Config {
    /// Read a user file. An explicit `--config` that is absent or malformed is an
    /// error - a typo'd path must fail rather than silently serve the defaults.
    pub(crate) fn read_toml(path: &std::path::Path) -> Result<ConfigToml, String> {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("read config {}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("parse config {}: {e}", path.display()))
    }
}

/// Overlay one file shape over another, field by field; `over` wins.
///
/// This is how the CLI outranks the file without a second merge mechanism - the CLI is
/// converted to a `ConfigToml` and laid over the file's.
pub(crate) fn overlay_toml(
    over: ConfigToml,
    under: ConfigToml,
) -> ConfigToml {
    ConfigToml {
        id: over.id.or(under.id),
        namespace: over.namespace.or(under.namespace),
        work_dir: over.work_dir.or(under.work_dir),
        deny: over.deny.or(under.deny),
        channel: match (over.channel, under.channel) {
            (Some(a), Some(b)) => Some(ChannelConfigToml {
                cert_dir: a.cert_dir.or(b.cert_dir),
                cert_name: a.cert_name.or(b.cert_name),
                bind: a.bind.or(b.bind),
                verify_timeout_secs: a.verify_timeout_secs.or(b.verify_timeout_secs),
            }),
            (a, b) => a.or(b),
        },
    }
}

/// The invoking user's name, for the zero-config human case. Harness `.mcp.json`
/// entries always pass `--id` explicitly.
fn default_id() -> String {
    std::env::var("USER").unwrap_or_else(|_| "default".to_string())
}

fn default_work_dir(id: &str) -> PathBuf {
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
