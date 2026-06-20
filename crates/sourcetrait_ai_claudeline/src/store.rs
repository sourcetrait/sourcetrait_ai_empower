use crate::*;

/// What: the statusline artifact directory,
/// `<XDG_CACHE>/sourcetrait/empower/statusline/`.
///
/// Why: regenerable per-session payloads belong in XDG cache, namespaced
/// under the sourcetrait vendor + the empower product (the same vendor
/// segment nushell_mcp uses). Where: persist / clear_latest.
fn statusline_dir() -> ClaudelineResult<PathBuf> {
    let base = directories::BaseDirs::new().ok_or(ClaudelineError::NoCacheDir)?;
    Ok(base
        .cache_dir()
        .join(lib::consts::SOURCETRAIT)
        .join("empower")
        .join("statusline"))
}

/// What: write the full payload (with an injected top-level `session_nom`)
/// as YAML to `{nom}.yaml`, then point the `{sid}.yaml` and `latest.yaml`
/// relative symlinks at it.
///
/// Why: gives the agent a stable, deterministically-named artifact plus a
/// `latest.yaml` pointer it can read directly - no glob-and-sort to find
/// the newest. The real file is named by the session nom; the SID symlink
/// correlates the two ids.
///
/// Where: run(), when the payload carries a session id.
pub(crate) fn persist(input: &Input, sid: &str) -> ClaudelineResult<()> {
    let nom = lib::ClaudeSessionNom::from(sid);
    let dir = statusline_dir()?;
    fs::create_dir_all(&dir).context(FsSnafu { path: dir.clone() })?;

    let mut value = input.value.clone();
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "session_nom".to_string(),
            serde_json::Value::String(nom.to_string()),
        );
    }
    let yaml = serde_norway::to_string(&value).context(SerializeYamlSnafu)?;

    let real_name = format!("{nom}.yaml");
    let real_path = dir.join(&real_name);
    fs::write(&real_path, yaml).context(FsSnafu {
        path: real_path.clone(),
    })?;

    relink(&dir.join(format!("{sid}.yaml")), &real_name)?;
    relink(&dir.join("latest.yaml"), &real_name)?;
    Ok(())
}

/// What: remove the `latest.yaml` pointer if it exists.
///
/// Why: a render with no session id must not leave a stale `latest.yaml`
/// pointing at another session. Where: run(), the no-SID path.
pub(crate) fn clear_latest() -> ClaudelineResult<()> {
    let link = statusline_dir()?.join("latest.yaml");
    unlink_if_present(&link)
}

/// What: (re)create `link` as a relative symlink to `target` (a sibling
/// filename), removing any existing link first.
///
/// Why: re-pointing each render keeps `latest.yaml` / `{sid}.yaml` current;
/// the target is the bare filename so neither end carries an absolute path.
/// Where: persist.
fn relink(link: &Path, target: &str) -> ClaudelineResult<()> {
    unlink_if_present(link)?;
    symlink(target, link).context(FsSnafu {
        path: link.to_path_buf(),
    })
}

/// What: remove a path if it exists, lstat-guarded so a dangling symlink
/// (whose target is gone) is still removed.
///
/// Why: `Path::exists` follows symlinks and reads a dangling link as
/// absent, which would skip the remove and make the next symlink() fail
/// with "File exists"; `symlink_metadata` lstats the link itself. Where:
/// relink, clear_latest.
fn unlink_if_present(link: &Path) -> ClaudelineResult<()> {
    if fs::symlink_metadata(link).is_ok() {
        fs::remove_file(link).context(FsSnafu {
            path: link.to_path_buf(),
        })?;
    }
    Ok(())
}
