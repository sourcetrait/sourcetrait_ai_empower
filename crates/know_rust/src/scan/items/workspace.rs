use crate::*;

/// What: orchestrate the workspace walk for `know_rust scan items`.
/// Walk every non-test, non-bench `.rs` file under `workspace_root`,
/// parse it via `syn::parse_file`, drive a `FileWalker` over the AST,
/// and merge each file's facts into the aggregate `ItemFacts`
/// returned to the caller.
///
/// Why: characterize.py consumes the produced JSON file as its
/// per-file lex+structure facts source. Separating walk from I/O lets
/// the entry point (`scan_items` in `scan::items::run`) own the
/// serialization + write step and lets integration tests assert
/// against the returned `ItemFacts` directly without rebinding via
/// JSON.
///
/// Where: called from `scan::items::run::scan_items` with the
/// cli-supplied workspace_root.
pub(crate) fn scan_workspace(workspace_root: &Path) -> ItemFacts {
    let mut facts = ItemFacts {
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        ..Default::default()
    };
    let mut aggregate_seams: HashMap<SeamKind, usize> = HashMap::new();
    let cfg_test_skips = collect_cfg_test_module_skips(workspace_root);
    let rs_files = collect_rs_files(workspace_root, &cfg_test_skips);
    for (path, rel) in &rs_files {
        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let parsed: syn::File = match syn::parse_str(&src) {
            Ok(f) => f,
            Err(_) => {
                facts.files_parse_failed += 1;
                continue;
            }
        };
        facts.files_scanned += 1;
        let walker = FileWalker::new(rel.clone());
        let file_facts = walker.walk_file(&parsed);
        merge_file_facts(&mut facts, &mut aggregate_seams, file_facts);
    }
    let mut seams_wire: BTreeMap<String, usize> = BTreeMap::new();
    for (k, n) in aggregate_seams {
        if n > 0 {
            seams_wire.insert(k.wire_key().to_string(), n);
        }
    }
    facts.seams = seams_wire;
    // R2-expansion 2026-06-05: dedup carry by name within each pattern
    // key. The walker pushes one entry per occurrence; the reader's
    // reference set is a SET of distinct dependent names. The pick's
    // own intra/inter counts already carry usage signal, so multi-site
    // occurrence in carry is redundant. Keeps first-occurrence order
    // for stable JSON output.
    for entries in facts.carries.values_mut() {
        let mut seen: HashSet<String> = HashSet::new();
        entries.retain(|e| seen.insert(e.name.clone()));
    }
    facts
}

/// What: collect every `.rs` file under `workspace_root`, sorted by
/// relative path for determinism. Skips `target/`, `.git/`, `tests/`,
/// and `benches/` segments anywhere in the path, plus the out-of-line
/// `#[cfg(test)] mod` files / dirs in `cfg_test_skips`.
///
/// Why: the items walker emits per-file facts; collecting the file
/// list up front keeps the orchestrator simple and lets the walker
/// loop iterate deterministically.
///
/// Where: called from `scan_workspace`'s top-of-fn file enumeration.
fn collect_rs_files(
    workspace_root: &Path,
    cfg_test_skips: &[PathBuf],
) -> Vec<(PathBuf, String)> {
    let mut rs_files = Vec::new();
    for entry in walkdir::WalkDir::new(workspace_root)
        .into_iter()
        .filter_entry(|e| !is_skip_dir(e.path()))
        .filter_map(|e| e.ok())
    {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        if p.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        if is_cfg_test_module_path(p, cfg_test_skips) {
            continue;
        }
        let rel = match p.strip_prefix(workspace_root) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };
        let parts: Vec<&str> = rel.split('/').collect();
        if parts.iter().any(|s| *s == "tests" || *s == "benches") {
            continue;
        }
        rs_files.push((p.to_path_buf(), rel));
    }
    rs_files.sort_by(|a, b| a.1.cmp(&b.1));
    rs_files
}

/// What: drain a per-file accumulator into the workspace-level
/// `ItemFacts` and merge its seam counters into the workspace
/// `SeamKind` map.
///
/// Why: keeping the merge logic in one place lets the walker stay
/// focused on per-node emission and lets the aggregator handle the
/// fact-list extension + seam-counter merge symmetrically.
///
/// Where: called from `scan_workspace` once per processed file.
fn merge_file_facts(
    facts: &mut ItemFacts,
    aggregate_seams: &mut HashMap<SeamKind, usize>,
    file_facts: FileLevelFacts,
) {
    facts.impls.extend(file_facts.impls);
    facts.traits.extend(file_facts.traits);
    facts.types.extend(file_facts.types);
    facts.fns.extend(file_facts.fns);
    facts.mods.extend(file_facts.mods);
    facts.uses.extend(file_facts.uses);
    facts.macros.extend(file_facts.macros);
    facts.macro_defs.extend(file_facts.macro_defs);
    facts.attrs.extend(file_facts.attrs);
    facts.derives.extend(file_facts.derives);
    facts.type_usages.extend(file_facts.type_usages);
    facts.example_type_usages.extend(file_facts.example_type_usages);
    facts.doc_count += file_facts.doc_count;
    for (kind, n) in file_facts.seams {
        *aggregate_seams.entry(kind).or_default() += n;
    }
    // R2 carry extraction: merge per-file carries into the workspace-
    // level BTreeMap keyed by `Pattern::Display` form. Multiple files
    // contributing to the same pattern (e.g. impl blocks for the same
    // struct in different modules) append their carry lists.
    for (pat, entries) in file_facts.carries {
        facts.carries.entry(pat).or_default().extend(entries);
    }
}
