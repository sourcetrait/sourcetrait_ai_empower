use crate::*;

/// One plugin as a positional `[name, version]` pair (version may be null).
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct PluginInfo(pub String, pub Option<String>);

pub(crate) fn registry_path() -> Option<PathBuf> {
    let config_dir = nu::nu_config_dir()?;
    Some(config_dir.join("plugin.msgpackz").into())
}

pub(crate) fn read_registry() -> Option<nu::PluginRegistryFile> {
    let path = registry_path()?;
    let mut file = fs::File::open(&path).ok()?;
    nu::PluginRegistryFile::read_from(&mut file, None).ok()
}

/// Last-modified time of the plugin registry file, or None when it is absent.
/// The EmbedEngine executor snapshots this when it builds the stateless base and
/// re-stats it per dispatch (~sub-microsecond): a change - a `plugin add/rm` via
/// interact() OR an external edit from the user's own shell - means the base's
/// plugin decls are stale, so the base is rebuilt and the ready-pool re-cloned.
/// mtime (not content) is the signal because it catches BOTH change sources,
/// which command-interception could not.
pub(crate) fn registry_mtime() -> Option<SystemTime> {
    let path = registry_path()?;
    fs::metadata(&path).and_then(|m| m.modified()).ok()
}

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
            "grammar: {error_count} plugin(s) failed to load from {}; see preceding error reports",
            path.display(),
        );
    }
    let delta = working_set.render();
    let _ = engine_state.merge_delta(delta);
}

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
