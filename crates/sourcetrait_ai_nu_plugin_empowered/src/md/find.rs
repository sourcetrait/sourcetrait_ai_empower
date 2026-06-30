use crate::*;

/// Find all regex matches in a markdown file and return their byte offsets as
/// (offset, length) pairs. Multiline mode is on by default so `^` and `$`
/// anchor at line boundaries.
///
/// Moved in from `sourcetrait_ai_lib_empower` when lib_empower dissolved; the
/// `empowered eye md find` command is the sole consumer.
pub(crate) fn find(
    path: &Path,
    pattern: &str,
) -> MarkdownResult<Vec<(usize, usize)>> {
    let content =
        fs::read_to_string(path).context(ReadFileSnafu { path: path.to_path_buf() })?;
    let re = regex::RegexBuilder::new(pattern)
        .multi_line(true)
        .build()
        .context(InvalidPatternSnafu {
            pattern: pattern.to_string(),
        })?;
    Ok(re.find_iter(&content).map(|m| (m.start(), m.len())).collect())
}
