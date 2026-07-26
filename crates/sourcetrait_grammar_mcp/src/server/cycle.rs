use crate::*;

/// How many times one file must be registered by a SINGLE parse before it is a cycle
/// rather than an unusual import graph. A legitimate tree registers each file once;
/// three is already impossible without resolution doubling back.
const CYCLE_REPEATS: usize = 3;

/// How many files a cycle message names before it stops listing them.
const CYCLE_NAMED: usize = 4;

/// The files a module-resolution CYCLE ran through, recognised from what a failed parse
/// registered; None when the parse failed for any other reason.
///
/// ## DEV
/// nushell HAS a circular-import detector - `FileStack::push` - and it does not fire
/// here. It compares raw `PathBuf`s, and an interior `..` is never folded away, because
/// folding it lexically is unsound the moment a symlink is involved (`a/..` is the
/// parent of the TARGET, not of the link). So a module reaching back into the cascade it
/// lives under presents a textually new path on every hop - `i/../i/../i/…` - and
/// resolution runs away until the accumulated path stops resolving.
///
/// The error that finally surfaces is NOT stable, which is why this keys on none of
/// them: a bare cycle reports `ModuleNotFound("../mod.nu")`, while the same cycle
/// carrying a couple of `export def`s reports `ModuleMissingModNuFile` with a
/// several-thousand-character path. Matching either one silently misses the other.
///
/// What IS invariant is the runaway itself - the same file registered over and over
/// under different spellings. Canonicalizing collapses them, and we can do that soundly
/// where nushell cannot, because we ask the filesystem rather than folding text.
///
/// `baseline` is `num_files()` taken BEFORE the parse, so only this parse's own
/// registrations count. Without it the interact lane, whose working set accumulates
/// across calls, would eventually look like a cycle on its own.
/// ##
pub(crate) fn detect_import_cycle(
    working_set: &nu::StateWorkingSet,
    baseline: usize,
) -> Option<String> {
    let mut counts: HashMap<PathBuf, usize> = HashMap::new();
    for cached in working_set.files().skip(baseline) {
        // A synthetic name (the submitted body) canonicalizes to nothing and drops out
        // here, which is what leaves only real files on disk to be counted.
        let Ok(canonical) = std::fs::canonicalize(&*cached.name) else {
            continue;
        };
        *counts.entry(canonical).or_default() += 1;
    }

    let mut repeated: Vec<(PathBuf, usize)> = counts
        .into_iter()
        .filter(|(_, seen)| *seen >= CYCLE_REPEATS)
        .collect();
    if repeated.is_empty() {
        return None;
    }
    // Most-repeated first, then by path, so one cycle always renders the same way.
    repeated.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let named: Vec<String> = repeated
        .iter()
        .take(CYCLE_NAMED)
        .map(|(path, _)| path.display().to_string())
        .collect();
    // The files alone. The KIND (`module::circular_import`) carries what happened, so
    // restating it here would only put prose on the wire; `<a> uses <b>` is also
    // nushell's own phrasing for this condition where its own detector does fire.
    Some(named.join(" uses "))
}
