use crate::*;

/// One plugin as a positional `[name, version]` pair (version may be null).
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct PluginInfo(pub String, pub Option<String>);

/// What: canonical path of the plugin registry file
/// (`<nu_config_dir>/plugin.msgpackz`). Returns None when no config
/// dir could be resolved (rare; missing $HOME or similar).
///
/// Why: centralizes the path resolution so both the worker's plugin-
/// loading path and the host's `info()` enumeration share the same
/// view of which file is canonical. `nu_path::nu_config_dir()` is
/// the same function `$nu.plugin-path` uses.
///
/// Where: called by `worker::base::load_plugins_best_effort` to set
/// `engine_state.plugin_path` and to open the registry, and by
/// `plugins::read_registry` to locate the file before opening.
pub(crate) fn registry_path() -> Option<PathBuf> {
    let config_dir = nu::nu_config_dir()?;
    Some(config_dir.join("plugin.msgpackz").into())
}

/// What: open the canonical plugin registry file and deserialize it
/// into a `PluginRegistryFile`. Returns None on any failure (no
/// config dir, file missing, brotli/msgpack decode error). The
/// silent-skip discipline is shared with `load_plugins_best_effort`:
/// missing or malformed registry leaves the host usable for non-
/// plugin code.
///
/// Why: both `load_plugins_best_effort` (which registers decls into
/// a working set) and `list_registered_plugins` (which projects to
/// names + versions) need the parsed file. Centralizing the read
/// keeps the disk + decode concerns in one place; callers pick what
/// to do with the parsed value.
///
/// Where: called by `worker::base::load_plugins_best_effort` and
/// `plugins::list_registered_plugins`.
pub(crate) fn read_registry() -> Option<nu::PluginRegistryFile> {
    let path = registry_path()?;
    let mut file = fs::File::open(&path).ok()?;
    nu::PluginRegistryFile::read_from(&mut file, None).ok()
}

/// What: enumerate `(name, version?)` records for every plugin in
/// the canonical registry. Returns an empty list when the registry
/// is missing or unreadable.
///
/// Why: host-side projection for `info()`'s `plugins` field.
/// Independent of any worker; pure function over the registry file.
/// Each `PluginRegistryItem`'s `data` is either `Valid { metadata,
/// commands }` (where `metadata.version: Option<String>` may carry
/// the plugin's self-reported version) or `Invalid` (deserialization
/// failed, no version recoverable).
///
/// Where: called by `NuSh::info` to populate the envelope's
/// `plugins` field.
pub(crate) fn list_registered_plugins() -> Vec<PluginInfo> {
    read_registry()
        .map(|f| {
            f.plugins
                .iter()
                .map(|p| {
                    let version = match &p.data {
                        nu::PluginRegistryItemData::Valid { metadata, .. } => {
                            metadata.version.clone()
                        }
                        nu::PluginRegistryItemData::Invalid => None,
                    };
                    PluginInfo(p.name.clone(), version)
                })
                .collect()
        })
        .unwrap_or_default()
}
