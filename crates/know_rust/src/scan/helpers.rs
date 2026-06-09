use crate::*;

/// What: path predicate that returns true when any path component is
/// `target` or `.git`, signaling the workspace walkers should skip the
/// subtree entirely.
///
/// Why: `target/` holds cargo build artifacts (recompilable /
/// generated .rs); `.git/` holds version-control internals (no
/// source). Both scan branches (items + usages) apply the same skip
/// set, so the predicate lives in shared scan helpers and both
/// walkers call into it via `use crate::*`.
///
/// Where: called from `scan::items::workspace::collect_rs_files`'s
/// `WalkDir::filter_entry` and from `scan::usages::walk::walk_workspace`'s
/// `WalkDir::filter_entry`.
pub(crate) fn is_skip_dir(p: &Path) -> bool {
    p.components().any(|c| {
        let name = c.as_os_str().to_str();
        name == Some("target") || name == Some(".git")
    })
}

/// What: collect filesystem prefixes for OUT-OF-LINE test modules -
/// the file / directory a `#[cfg(test)] mod <name>;` declaration
/// resolves to. Returns the skip prefixes shared by both walkers and
/// the per-crate SLOC walk.
///
/// Why: "no tests" is the standing design rule (the_user, 0.0.25
/// era). The dir-name skip (tests/ + benches/) and the inline
/// cfg(test)-item skip both miss the out-of-line form: a file like
/// `src/bundle/tests.rs` carries no cfg attrs itself - the gate
/// lives on the `mod` declaration in the parent file - so it scanned
/// as production code (R7 topic n: bevy bundle/tests.rs, ratatui
/// backend/test.rs, nushell test_util.rs). Following the declaration
/// is the language-semantic rule; a file-name list would be a proxy.
///
/// Textual pre-pass (regex per source file) rather than a parse
/// pass: runs independent of syn and tolerates files that fail to
/// parse. Conventional attr stacks (`#[cfg(test)]` followed by other
/// attrs, optional `pub`) are recognized; cfg_attr indirection is
/// out of scope.
///
/// Where: called from `scan::items::workspace::scan_workspace`,
/// `scan::usages::walk::walk_workspace`, and
/// `characterize::run::characterize` (which threads the set into
/// `scan_crate` for SLOC parity).
pub(crate) fn collect_cfg_test_module_skips(root: &Path) -> Vec<PathBuf> {
    let re = regex::Regex::new(
        r"(?s)#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_]\w*)\s*;",
    )
    .expect("static regex compiles");
    let mut skips: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_skip_dir(e.path()))
        .filter_map(|e| e.ok())
    {
        let p = entry.path();
        if !p.is_file() || p.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let src = match fs::read_to_string(p) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if !src.contains("#[cfg(test)]") {
            continue;
        }
        let parent = match p.parent() {
            Some(d) => d.to_path_buf(),
            None => continue,
        };
        let stem = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        // mod resolution base: mod.rs / lib.rs / main.rs declare
        // siblings; a 2018-style `foo.rs` declares under `foo/`.
        let base = if stem == "mod" || stem == "lib" || stem == "main" {
            parent
        } else {
            parent.join(&stem)
        };
        for cap in re.captures_iter(&src) {
            let m = &cap[1];
            skips.push(base.join(format!("{}.rs", m)));
            skips.push(base.join(m));
        }
    }
    skips
}

/// What: true when `p` equals a skip file or sits under a skip
/// directory prefix from `collect_cfg_test_module_skips`.
pub(crate) fn is_cfg_test_module_path(p: &Path, skips: &[PathBuf]) -> bool {
    skips.iter().any(|s| p == s || p.starts_with(s))
}

/// What: true when an attribute list carries a bare `#[cfg(test)]`.
///
/// Why: the usages scanner walks items without consulting attrs, so
/// inline `#[cfg(test)] mod t { ... }` contents fed fn-sig / field /
/// method-ref signals (the items walker already skips them via
/// `process_item_attrs`). Shared predicate for that parity guard.
///
/// Where: called from `scan::usages::scan::walk_items`.
pub(crate) fn has_cfg_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path()
            .segments
            .last()
            .map(|s| s.ident == "cfg")
            .unwrap_or(false)
            && match &a.meta {
                syn::Meta::List(list) => list.tokens.to_string().trim() == "test",
                _ => false,
            }
    })
}

/// What: render a `syn::Visibility` as the wire string the walkers
/// emit (`""` inherited, `"pub"` plain, `"pub(path)"` restricted).
///
/// Why: visibility strings cross the items walker boundary into
/// per-fact emission AND into the usages walker's fn_visibility /
/// container_visibility / field_visibility / alias_visibility fields,
/// so the conversion sits in shared scan helpers and both walkers
/// call into it via `use crate::*`.
///
/// Where: called from `scan::items::walker::FileWalker` and from
/// `scan::usages::scan`'s walk_fn / walk_impl / walk_trait /
/// emit_field / emit_field_unnamed / walk_type_alias.
pub(crate) fn visibility_string(vis: &syn::Visibility) -> String {
    match vis {
        syn::Visibility::Public(_) => "pub".to_string(),
        syn::Visibility::Restricted(r) => {
            let path = r
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            format!("pub({})", path)
        }
        syn::Visibility::Inherited => String::new(),
    }
}
