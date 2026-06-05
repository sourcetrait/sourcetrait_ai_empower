use crate::*;

/// What: orchestrate the `know_rust emit` invocation. Read
/// `fingerprint.json` + `facts.json` from the output directory,
/// build the orientation map + the exhaustive reference index,
/// render via liquid templates, and write `orientation.md` +
/// `reference.md` to the same output directory.
///
/// Why: emit.py phase 3 port. Reads characterize's output, produces
/// the agent-facing artifact. Per the_user 2026-06-05: all
/// mechanically-generated prose moves to liquid templates so
/// iteration on prompt design doesn't require Rust recompiles.
///
/// Where: dispatched by `crate::run::run` via the `ScanCommand`
/// peer `Command::Emit`; called from
/// `tests/emit_integration.rs` when phase 6 lands. Phase 3a foundation
/// here writes minimal stub output; phase 3b fills in picker +
/// sections + reference renderer + templates.
pub fn emit(
    workspace_root: &Path,
    out_dir: &Path,
    _calibration: &Calibration,
    _templates: &Templates,
) -> std::result::Result<(), Error> {
    let fp_path = out_dir.join("fingerprint.json");
    let facts_path = out_dir.join("facts.json");
    let fp_text = fs::read_to_string(&fp_path).map_err(|source| Error::Read {
        path: fp_path.clone(),
        source,
    })?;
    let facts_text = fs::read_to_string(&facts_path).map_err(|source| Error::Read {
        path: facts_path.clone(),
        source,
    })?;
    let _fp: serde_json::Value = serde_json::from_str(&fp_text)
        .map_err(|source| Error::Serialize { source })?;
    let _facts: serde_json::Value = serde_json::from_str(&facts_text)
        .map_err(|source| Error::Serialize { source })?;

    let orientation_path = out_dir.join("orientation.md");
    let reference_path = out_dir.join("reference.md");
    fs::write(
        &orientation_path,
        format!("# Orientation\n\n(phase 3 stub - {} workspace)\n", workspace_root.display()),
    )
    .map_err(|source| Error::Write {
        path: orientation_path,
        source,
    })?;
    fs::write(
        &reference_path,
        "# Reference Index\n\n(phase 3 stub)\n",
    )
    .map_err(|source| Error::Write {
        path: reference_path,
        source,
    })?;

    eprintln!("[emit] wrote orientation.md + reference.md to {}", out_dir.display());
    Ok(())
}
