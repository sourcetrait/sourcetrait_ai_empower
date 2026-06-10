use crate::*;

/// What: orchestrate the `know_rust emit` invocation. Read
/// `fingerprint.json` + `facts.json` from the output directory, render
/// the orientation.md + reference.md artifacts, dispatch between the
/// standard orientation composer and the container-routing composer
/// based on `fingerprint.workspace_shape.shape`, and write the
/// resulting files back to the output directory.
///
/// Why: emit.py phase 3 port. Reads characterize's output, produces
/// the agent-facing artifact pair. The container-shape carve-out lives
/// here so `render_orientation` does not need to handle the routing
/// path internally.
///
/// Where: dispatched by `crate::run::run` via `Command::Emit`. Phase
/// 3d wiring lands the orientation composer + container routing on top
/// of the already-shipped reference renderer; phase 3e moves the prose
/// to liquid templates.
pub fn emit(
    workspace_root: &Path,
    out_dir: &Path,
    calibration: &Calibration,
    templates: &Templates,
    weights: Option<&WeightBlob>,
    profile_name: &str,
) -> std::result::Result<(), Error> {
    // Resolve the documentation-kind profile up front so a typo'd
    // name fails before any artifact is written.
    let profile = calibration.resolve_profile(profile_name)?;
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
    let fp: serde_json::Value =
        serde_json::from_str(&fp_text).map_err(|source| Error::Serialize { source })?;
    let facts: serde_json::Value =
        serde_json::from_str(&facts_text).map_err(|source| Error::Serialize { source })?;

    let orientation_path = out_dir.join("orientation.md");
    let reference_path = out_dir.join("reference.md");
    let reference_text = render_reference(workspace_root, out_dir, &fp, &facts);
    let shape = fp
        .get("workspace_shape")
        .and_then(|v| v.get("shape"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    // Resolve which blob target applies to this workspace (basename
    // first, else a crate-name match); container shapes skip the
    // picker entirely so weights never apply there.
    let crate_names: Vec<String> = fp
        .get("per_crate")
        .and_then(|v| v.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    let tweights = weights.and_then(|b| target_weights(b, workspace_root, &crate_names));
    let orientation_text = if shape == "container" {
        render_container_routing(workspace_root, out_dir, &fp, templates)
    } else {
        render_orientation(
            workspace_root,
            out_dir,
            &fp,
            &facts,
            calibration,
            templates,
            tweights,
            profile_name,
            &profile,
        )
    };
    fs::write(&orientation_path, &orientation_text).map_err(|source| Error::Write {
        path: orientation_path,
        source,
    })?;
    fs::write(&reference_path, reference_text).map_err(|source| Error::Write {
        path: reference_path,
        source,
    })?;

    let orient_nl = orientation_text.matches('\n').count();
    eprintln!(
        "[emit] wrote orientation.md ({} lines) and reference.md to {}",
        orient_nl,
        out_dir.display()
    );
    Ok(())
}
