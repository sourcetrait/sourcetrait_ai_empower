use crate::*;

/// How many times one file must be registered before it is a cycle.
const CYCLE_REPEATS: usize = 3;

/// How many files a cycle message names before it stops listing them.
const CYCLE_NAMED: usize = 4;

/// The files a module-resolution CYCLE ran through; None on any other failure.
pub(crate) fn detect_import_cycle(
    working_set: &nu::StateWorkingSet,
    baseline: usize,
) -> Option<String> {
    let mut counts: HashMap<PathBuf, usize> = HashMap::new();
    for cached in working_set.files().skip(baseline) {
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
    repeated.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let named: Vec<String> = repeated
        .iter()
        .take(CYCLE_NAMED)
        .map(|(path, _)| path.display().to_string())
        .collect();
    Some(named.join(" uses "))
}
