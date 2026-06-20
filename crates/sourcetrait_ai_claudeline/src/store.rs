use crate::*;

/// What: this identity's claudeline cache root,
/// `<XDG_CACHE>/sourcetrait/empower/claudeline/<identity>/`.
///
/// Why: per-identity namespacing keeps multiple harnesses on one box from
/// colliding; the sourcetrait/empower segments match the vendor + product
/// the rest of the cache uses.
///
/// Where: status_dir / context_dir.
fn claudeline_base(identity: &str) -> ClaudelineResult<PathBuf> {
    let base = directories::BaseDirs::new().ok_or(ClaudelineError::NoCacheDir)?;
    Ok(base
        .cache_dir()
        .join(lib::consts::SOURCETRAIT)
        .join("empower")
        .join("claudeline")
        .join(identity))
}

/// What: the full-payload sub-dir, `<base>/status/`.
///
/// Why: the lossless mirror + its pointers live apart from the minimized
/// context artifact. Where: persist_status, read_prev_session, prune.
fn status_dir(identity: &str) -> ClaudelineResult<PathBuf> {
    Ok(claudeline_base(identity)?.join("status"))
}

/// What: the minimized-artifact sub-dir, `<base>/context/`.
///
/// Why: the threshold read is a separate, smaller file from the full status
/// mirror. Where: persist_context, prune.
fn context_dir(identity: &str) -> ClaudelineResult<PathBuf> {
    Ok(claudeline_base(identity)?.join("context"))
}

/// What: the previous render's session ids, scanned out of the existing
/// status/latest.yaml before this render writes anything.
///
/// Why: a new session is "this run's sid differs from the one latest.yaml
/// still points at"; that prior session is the one the prune keeps alongside
/// the current. Where: read_prev_session, persist_session.
struct PrevSession {
    sid: String,
    nom: String,
}

/// What: the whole cache side-write for a render that carries a session id -
/// read the previous session, write the status mirror, build + write the
/// context artifact (or the schema-change canary), and prune to {current,
/// previous} on a new-session transition.
///
/// Why: one entry point owns the ordering rules - read-prev BEFORE any write,
/// status BEFORE the Input-consuming context build, prune LAST and only on a
/// confidently-detected new session. Best-effort: run() swallows the error
/// so the render never fails.
///
/// Where: run(), when both a session id and an identity are present.
pub(crate) fn persist_session(
    input: Input,
    sid: &str,
    identity: &str,
) -> ClaudelineResult<()> {
    let prev = read_prev_session(identity);
    let nom = lib::ClaudeSessionNom::from(sid);
    persist_status(&input, sid, &nom, identity)?;
    persist_context(ContextModel::try_from(input), &nom, identity)?;
    if let Some(prev) = prev {
        if prev.sid != sid {
            prune(&[nom.to_string(), prev.nom], identity)?;
        }
    }
    Ok(())
}

/// What: write the full payload (with an injected top-level `session_nom`)
/// as YAML to status/<nom>.yaml, then point status/<sid>.yaml and
/// status/latest.yaml at it (relative symlinks).
///
/// Why: the lossless mirror is the agent's full-introspection artifact and
/// stays resilient to upstream schema drift - it mirrors whatever valid JSON
/// arrives. Where: persist_session, before the context build.
fn persist_status(
    input: &Input,
    sid: &str,
    nom: &lib::ClaudeSessionNom,
    identity: &str,
) -> ClaudelineResult<()> {
    let dir = status_dir(identity)?;
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

/// What: write the minimized context model to context/<nom>.yaml and point
/// context/latest.yaml at it; on a schema-change failure write the degraded
/// canary artifact (`error: statusline JSON schema has changed`) instead.
///
/// Why: context/ is the agent's cheap threshold read and the typed canary
/// for the fields it depends on; the degraded artifact is valid YAML
/// (`{error: ...}`) so the agent sees the drift the moment it reads
/// latest.yaml. Where: persist_session, after persist_status.
fn persist_context(
    model: Result<ContextModel, ContextSchemaChanged>,
    nom: &lib::ClaudeSessionNom,
    identity: &str,
) -> ClaudelineResult<()> {
    let dir = context_dir(identity)?;
    fs::create_dir_all(&dir).context(FsSnafu { path: dir.clone() })?;

    let yaml = match model {
        Ok(model) => serde_norway::to_string(&model).context(SerializeYamlSnafu)?,
        Err(_) => "error: statusline JSON schema has changed\n".to_string(),
    };

    let real_name = format!("{nom}.yaml");
    let real_path = dir.join(&real_name);
    fs::write(&real_path, yaml).context(FsSnafu {
        path: real_path.clone(),
    })?;

    relink(&dir.join("latest.yaml"), &real_name)?;
    Ok(())
}

/// What: scan the existing status/latest.yaml for its top-level
/// `session_id:` / `session_nom:` values via a buffered line read, stopping
/// once both are found; None if there is no prior file or either key is
/// absent.
///
/// Why: cheap (no YAML parse) and it must run BEFORE any write, while
/// latest.yaml still points at the prior session - that prior session is
/// what a new render prunes around. Where: persist_session, first step.
fn read_prev_session(identity: &str) -> Option<PrevSession> {
    let path = status_dir(identity).ok()?.join("latest.yaml");
    let reader = io::BufReader::new(fs::File::open(&path).ok()?);
    let mut sid = None;
    let mut nom = None;
    for line in reader.lines() {
        let line = line.ok()?;
        if let Some(value) = top_level_scalar(&line, "session_id:") {
            sid = Some(value);
        } else if let Some(value) = top_level_scalar(&line, "session_nom:") {
            nom = Some(value);
        }
        if sid.is_some() && nom.is_some() {
            break;
        }
    }
    Some(PrevSession {
        sid: sid?,
        nom: nom?,
    })
}

/// What: if `line` is the unindented YAML key `key`, return its trimmed,
/// unquoted scalar value; None otherwise.
///
/// Why: the rapid session scan keys off the column-zero session_id /
/// session_nom lines without parsing YAML; the bare-key prefix match rejects
/// indented (nested) keys by construction. Where: read_prev_session.
fn top_level_scalar(
    line: &str,
    key: &str,
) -> Option<String> {
    let value = line.strip_prefix(key)?;
    Some(value.trim().trim_matches('"').to_string())
}

/// What: delete every cache entry under this identity's status/ and context/
/// dirs whose session nom is not in `keep`.
///
/// Why: a new session keeps only {current, previous}; older sessions' real
/// files and pointer symlinks are dropped. Shaped around a nom keep-list so
/// later rules can extend what survives. Where: persist_session on a
/// new-session transition.
fn prune(
    keep: &[String],
    identity: &str,
) -> ClaudelineResult<()> {
    for dir in [status_dir(identity)?, context_dir(identity)?] {
        prune_dir(&dir, keep)?;
    }
    Ok(())
}

/// What: prune one directory - remove each entry whose nom is not kept (real
/// files by filename stem, symlinks by their target's stem).
///
/// Why: resolving symlinks by target means a kept session's <sid>.yaml and
/// latest.yaml survive automatically, while a dropped session's pointers and
/// dangling links go. A missing dir is a no-op. Where: prune.
fn prune_dir(
    dir: &Path,
    keep: &[String],
) -> ClaudelineResult<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };
    for entry in entries {
        let path = entry.context(FsSnafu { path: dir.to_path_buf() })?.path();
        if !keep_entry(&path, keep) {
            let _ = fs::remove_file(&path);
        }
    }
    Ok(())
}

/// What: should this entry survive the prune? A symlink survives iff its
/// target's nom is kept; a real file iff its own stem is kept; anything we
/// cannot stat is kept (do not drop what we do not understand).
///
/// Why: encodes the keep rule for both real <nom>.yaml files and the
/// <sid>.yaml / latest.yaml pointer symlinks. Where: prune_dir.
fn keep_entry(
    path: &Path,
    keep: &[String],
) -> bool {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(_) => return true,
    };
    if meta.file_type().is_symlink() {
        match fs::read_link(path) {
            Ok(target) => stem_kept(&target, keep),
            Err(_) => false,
        }
    } else {
        stem_kept(path, keep)
    }
}

/// What: is `path`'s file stem (the nom) in the keep list?
///
/// Why: both the real-file and symlink-target checks reduce to a stem
/// membership test. Where: keep_entry.
fn stem_kept(
    path: &Path,
    keep: &[String],
) -> bool {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| keep.iter().any(|k| k.as_str() == stem))
}

/// What: (re)create `link` as a relative symlink to the sibling filename
/// `target`, removing any existing link first.
///
/// Why: re-pointing each render keeps latest.yaml / <sid>.yaml current; the
/// bare-filename target keeps both ends free of absolute paths. Where:
/// persist_status, persist_context.
fn relink(
    link: &Path,
    target: &str,
) -> ClaudelineResult<()> {
    unlink_if_present(link)?;
    symlink(target, link).context(FsSnafu {
        path: link.to_path_buf(),
    })
}

/// What: remove a path if it exists, lstat-guarded so a dangling symlink
/// (whose target is gone) is still removed.
///
/// Why: Path::exists follows symlinks and reads a dangling link as absent,
/// which would skip the remove and make the next symlink() fail with "File
/// exists"; symlink_metadata lstats the link itself. Where: relink.
fn unlink_if_present(link: &Path) -> ClaudelineResult<()> {
    if fs::symlink_metadata(link).is_ok() {
        fs::remove_file(link).context(FsSnafu {
            path: link.to_path_buf(),
        })?;
    }
    Ok(())
}
