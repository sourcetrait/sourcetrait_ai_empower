use crate::*;

/// What: per-file lex+structure facts walker. Phase 3 stub: writes
/// an empty recon_items.json with the same top-level shape
/// rustscan.py emits per file (impls + traits + types + fns +
/// macros + macro_defs + attrs + derives + uses + mods +
/// type_usages + example_type_usages + seams + doc_count), all
/// empty/zero. Phase 4 ports rustscan.py logic via syn.
///
/// Why: clap surface needs the Items variant before phase 4's
/// implementation lands; phase 3 isolates the stub commit from the
/// substantive port to keep output-equivalence verification clean
/// at each phase boundary. characterize.py does not invoke
/// `scan items` until phase 5; this stub is only reachable via
/// direct CLI invocation during phase 3+4 development.
///
/// Where: invoked from run.rs dispatch_scan() when ScanCommand::Items
/// matches.
pub(crate) fn scan_workspace(
    _workspace_root: &Path,
    out_dir: &Path,
) -> std::result::Result<(), Error> {
    let stub = serde_json::json!({
        "tool_version": env!("CARGO_PKG_VERSION"),
        "files_scanned": 0,
        "files_parse_failed": 0,
        "impls": [],
        "traits": [],
        "types": [],
        "fns": [],
        "macros": [],
        "macro_defs": [],
        "attrs": [],
        "derives": [],
        "uses": [],
        "mods": [],
        "type_usages": [],
        "example_type_usages": [],
        "seams": {},
        "doc_count": 0,
    });
    let out_path = out_dir.join("recon_items.json");
    let json = serde_json::to_string_pretty(&stub)
        .map_err(|source| Error::Serialize { source })?;
    let write_path = out_path.clone();
    fs::write(&out_path, json).map_err(|source| Error::Write {
        path: write_path,
        source,
    })?;
    eprintln!(
        "[rust_recon scan items] phase 3 stub; wrote empty {}",
        out_path.display()
    );
    Ok(())
}
