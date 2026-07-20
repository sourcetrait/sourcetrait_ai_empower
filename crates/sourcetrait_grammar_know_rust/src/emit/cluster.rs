#[allow(unused_imports)]
use crate::*;

/// What: group workspace crate names by longest shared name prefix
/// (split on `_` or `-` independently) into `clusters` of size at
/// least `min_size` and `others` for leftovers. Snake-case and
/// kebab-case prefixes are kept distinct so `nu_plugin_*` and
/// `nu-plugin-*` form separate architectural clusters when both
/// conventions coexist.
///
/// Why: emit.py's `_cluster_crates_by_prefix()` (lines 260-303).
/// Workspaces with many small crates (nushell, helix, bevy at 70+
/// crates each) benefit from a prefix-grouped overview above the
/// per-crate detail; the cluster sub-section surfaces architectural
/// families the per-crate list flattens out.
///
/// Where: called by `crate::emit::orientation::render_orientation`
/// inside S1 (Crate / region map) when the workspace has more than
/// `calibration.picker.cluster.threshold` crates AND at least one
/// prefix group reaches `calibration.picker.cluster.min_size`.
pub fn cluster_crates_by_prefix(
    crate_names: &[String],
    min_size: usize,
) -> (indexmap::IndexMap<String, Vec<String>>, Vec<String>) {
    let mut crates: Vec<String> = crate_names.to_vec();
    crates.sort();

    let prefix_candidates = |name: &str| -> Vec<String> {
        let mut results: Vec<String> = Vec::new();
        for sep in ['_', '-'] {
            if !name.contains(sep) {
                continue;
            }
            let parts: Vec<&str> = name.split(sep).collect();
            for k in (1..parts.len()).rev() {
                let p = parts[..k].join(&sep.to_string());
                if !p.is_empty() && !results.contains(&p) {
                    results.push(p);
                }
            }
        }
        results
    };

    let mut prefix_members: indexmap::IndexMap<String, indexmap::IndexSet<String>> =
        indexmap::IndexMap::new();
    for name in &crates {
        for p in prefix_candidates(name) {
            prefix_members
                .entry(p)
                .or_default()
                .insert(name.clone());
        }
    }

    let mut clusters: indexmap::IndexMap<String, Vec<String>> = indexmap::IndexMap::new();
    let mut others: Vec<String> = Vec::new();
    for name in &crates {
        let mut assigned: Option<String> = None;
        for p in prefix_candidates(name) {
            if prefix_members
                .get(&p)
                .map(|s| s.len() >= min_size)
                .unwrap_or(false)
            {
                assigned = Some(p);
                break;
            }
        }
        match assigned {
            Some(p) => clusters.entry(p).or_default().push(name.clone()),
            None => others.push(name.clone()),
        }
    }
    (clusters, others)
}
