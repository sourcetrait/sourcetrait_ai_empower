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

/// What: register every plugin's decls from the canonical registry into
/// `engine_state`, and point `engine_state.plugin_path` at that registry (so
/// `$nu.plugin-path` reflects it once `generate_nu_constant` runs). Every step
/// is best-effort: no config dir, no file, a decode error, an individual plugin
/// load error -- all skip; a partial load prints a summary to stderr alongside
/// the per-plugin reports `load_plugin_file` already emits.
///
/// Why: BOTH sides need the same plugin decls, and this is the one place they
/// come from. The WORKER needs them so an agent body / call-target can invoke a
/// plugin command at all. The VALIDATOR needs them because a plugin command that
/// extends a BUILTIN family is otherwise REJECTED at commit -- `from empowered
/// liquid` fails `ExtraPositional("from ", ...)` since the parser binds the
/// builtin `from` and reads the rest as extra positionals -- while a plugin
/// command with its OWN head slips through unvalidated as an implicit external.
/// Sharing the loader means the two views cannot drift, the same reason
/// `registry_path` / `read_registry` live here.
///
/// Note this registers plugin DECLS; it spawns no plugin PROCESS (nushell's
/// plugin engine spawns those lazily on first invocation), so the validator pays
/// only the registry read.
///
/// Where: `worker::base::WarmBase::new` and
/// `server::parse_engine::ParseEngine::new_full`.
pub(crate) fn load_plugin_decls(engine_state: &mut nu::EngineState) {
    let Some(path) = registry_path() else {
        return;
    };
    engine_state.plugin_path = Some(path.clone().into());
    let Some(contents) = read_registry() else {
        return;
    };
    let mut working_set = nu::StateWorkingSet::new(engine_state);
    let error_count = nu::load_plugin_file(&mut working_set, &contents, None);
    if error_count > 0 {
        eprintln!(
            "nushell_mcp: {error_count} plugin(s) failed to load from {}; see preceding error reports",
            path.display(),
        );
    }
    let delta = working_set.render();
    let _ = engine_state.merge_delta(delta);
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
