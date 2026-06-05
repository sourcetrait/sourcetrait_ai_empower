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
