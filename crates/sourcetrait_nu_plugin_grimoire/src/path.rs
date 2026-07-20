use crate::*;

/// Expand a user-facing path against the caller's cwd: `~` and any relative path
/// resolve per the nu-plugin cwd = caller-cwd convention. No existence check -
/// use for a path that may not exist yet (e.g. an output target).
pub(crate) fn expand(
    engine: &nu::EngineInterface,
    raw: &Path,
) -> Result<PathBuf, nu::LabeledError> {
    let cwd = engine.get_current_dir()?;
    Ok(nu_path::expand_path_with(raw, cwd, true))
}

/// Expand (as above) then canonicalize a user-facing path that MUST exist:
/// canonicalize resolves symlinks + `..` and validates existence, so a missing
/// path surfaces as a clean error at the call head.
pub(crate) fn canonical(
    engine: &nu::EngineInterface,
    raw: &Path,
    head: nu::Span,
) -> Result<PathBuf, nu::LabeledError> {
    let expanded = expand(engine, raw)?;
    fs::canonicalize(&expanded)
        .map_err(|error| labeled_error(format!("{}: {error}", expanded.display()), head))
}
