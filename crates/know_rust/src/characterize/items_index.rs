use crate::*;

/// What: bucket the workspace-level `ItemFacts` into a per-file index
/// keyed by relative file path so `scan_crate` can drain per-file
/// facts via the same path keys the items walker emitted.
///
/// Why: characterize.py's `_build_items_index` does this same
/// rebucketing on the flat workspace-level json lists. Reusing the
/// already-typed `ItemFacts` directly (rather than re-reading
/// know_rust_items.json) is the natural rust-side simplification: the
/// items walker runs in-process and returns the typed struct, no
/// subprocess round-trip required.
///
/// Where: called from `crate::characterize::run::characterize` once
/// `scan::items::workspace::scan_workspace` returns the workspace
/// `ItemFacts`.
pub fn build_items_index(facts: &ItemFacts) -> std::collections::HashMap<String, ItemFile> {
    let mut by_file: std::collections::HashMap<String, ItemFile> =
        std::collections::HashMap::new();
    for rec in &facts.impls {
        by_file
            .entry(rec.file.clone())
            .or_default()
            .impls
            .push(rec.clone());
    }
    for rec in &facts.traits {
        by_file
            .entry(rec.file.clone())
            .or_default()
            .traits
            .push(rec.clone());
    }
    for rec in &facts.types {
        by_file
            .entry(rec.file.clone())
            .or_default()
            .types
            .push(rec.clone());
    }
    for rec in &facts.fns {
        by_file
            .entry(rec.file.clone())
            .or_default()
            .fns
            .push(rec.clone());
    }
    for rec in &facts.uses {
        by_file
            .entry(rec.file.clone())
            .or_default()
            .uses
            .push(rec.clone());
    }
    for rec in &facts.macros {
        by_file
            .entry(rec.file.clone())
            .or_default()
            .macros
            .push(rec.clone());
    }
    for rec in &facts.derives {
        by_file
            .entry(rec.file.clone())
            .or_default()
            .derives
            .push(rec.clone());
    }
    for rec in &facts.macro_defs {
        by_file
            .entry(rec.file.clone())
            .or_default()
            .macro_defs
            .push(rec.clone());
    }
    for rec in &facts.mods {
        by_file
            .entry(rec.file.clone())
            .or_default()
            .mods
            .push(rec.clone());
    }
    for rec in &facts.type_usages {
        by_file
            .entry(rec.file.clone())
            .or_default()
            .type_usages
            .push(rec.clone());
    }
    for rec in &facts.example_type_usages {
        by_file
            .entry(rec.file.clone())
            .or_default()
            .example_type_usages
            .push(rec.clone());
    }
    by_file
}
