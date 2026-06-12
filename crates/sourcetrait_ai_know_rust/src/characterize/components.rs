use crate::*;

/// What: union-find data structure over crate names supporting
/// `union(a, b)` and `find(x)` queries plus a final `components()`
/// roll-up to grouped sorted lists.
///
/// Why: characterize.py's connected-component grouping for the
/// dependency graph drives the `n_components` field + regional-mode
/// escalation in select_mode. Plain path-halving + union-by-default
/// implementation matches the python idiom.
///
/// Where: used by `compute_components` below; not exposed beyond
/// the characterize module.
pub(crate) struct UnionFind {
    parent: std::collections::HashMap<String, String>,
}

impl UnionFind {
    fn new<I: IntoIterator<Item = String>>(items: I) -> Self {
        let mut parent = std::collections::HashMap::new();
        for x in items {
            parent.insert(x.clone(), x);
        }
        Self { parent }
    }

    fn find(&mut self, x: &str) -> String {
        let mut current = x.to_string();
        while let Some(p) = self.parent.get(&current).cloned() {
            if p == current {
                return p;
            }
            let pp = self.parent.get(&p).cloned().unwrap_or_else(|| p.clone());
            self.parent.insert(current.clone(), pp.clone());
            current = pp;
        }
        current
    }

    fn union(&mut self, a: &str, b: &str) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent.insert(ra, rb);
        }
    }
}

/// What: compute connected components of the workspace dependency
/// graph restricted to internal dependencies (deps that name another
/// workspace crate). Returns a list of sorted crate-name lists, one
/// per component.
///
/// Why: matches characterize.py's `components(crates)`. The component
/// count signals regional vs monolithic workspace shape and is one
/// of the structural-regional gates in select_mode.
///
/// Where: called from `crate::characterize::run::characterize` after
/// `find_crates` populates the crate graph.
pub fn compute_components(
    crates: &indexmap::IndexMap<String, CrateInfo>,
) -> Vec<Vec<String>> {
    let internal: std::collections::HashSet<String> = crates.keys().cloned().collect();
    let mut uf = UnionFind::new(crates.keys().cloned());
    for (name, info) in crates {
        for dep in &info.deps {
            if internal.contains(dep) {
                uf.union(name, dep);
            }
        }
    }
    let mut groups: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for name in crates.keys() {
        let root = uf.find(name);
        groups.entry(root).or_default().push(name.clone());
    }
    let mut out: Vec<Vec<String>> = groups
        .into_values()
        .map(|mut g| {
            g.sort();
            g
        })
        .collect();
    out.sort();
    out
}
