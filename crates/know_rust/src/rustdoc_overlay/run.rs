use crate::*;

/// What: orchestrate the `know_rust rustdoc-overlay` subcommand. Hard-
/// requires a nightly cargo toolchain (no degrade banner per locked
/// decision 4 in conversion_status.md). Runs `cargo +nightly rustdoc
/// -p <pkg> --lib -- -Z unstable-options --output-format json`,
/// parses the resulting target/doc/<crate>.json, reconciles against
/// the characterize floor's facts.json, and writes the overlay to
/// `<orientation_dir>/rustdoc_overlay.json`.
///
/// Why: rustdoc_overlay.py's `main()` (lines 230-265) ported to Rust.
/// The hard-requirement variant matches the_user 2026-06-05 direction
/// (cargo + nightly are already runtime deps; nightly implies stable;
/// failure should surface, not silently degrade).
///
/// Where: dispatched by `crate::run::run` when
/// `know_rust rustdoc-overlay <repo_root> <orientation_dir>
/// [package]` is invoked.
pub fn rustdoc_overlay(
    root: &Path,
    orientation_dir: &Path,
    requested_package: Option<&str>,
) -> std::result::Result<(), Error> {
    ensure_nightly_toolchain()?;

    let facts_path = orientation_dir.join("facts.json");
    let facts_text = fs::read_to_string(&facts_path).map_err(|source| Error::Read {
        path: facts_path.clone(),
        source,
    })?;
    let facts: serde_json::Value =
        serde_json::from_str(&facts_text).map_err(|source| Error::Serialize { source })?;

    let resolved_package = resolve_package(root, requested_package, orientation_dir);
    let rustdoc = run_rustdoc_json(root, resolved_package.as_deref())?;
    let overlay = reconcile(&facts, &rustdoc)?;

    let overlay_path = orientation_dir.join("rustdoc_overlay.json");
    let overlay_json =
        serde_json::to_string_pretty(&overlay).map_err(|source| Error::Serialize { source })?;
    fs::write(&overlay_path, &overlay_json).map_err(|source| Error::Write {
        path: overlay_path,
        source,
    })?;
    println!(
        "[overlay] applied (format_version={}); {} macro/blanket items, {} disagreement(s).",
        overlay.format_version,
        overlay.macro_generated.len(),
        overlay.disagreements.len()
    );
    Ok(())
}

/// What: verify the nightly cargo toolchain is available. Hard-error
/// out per locked decision 4 if `cargo` is missing or `cargo +nightly
/// --version` fails.
///
/// Why: the_user 2026-06-05 directive: "we already have a runtime dep
/// on cargo and cargo nightly. nightly implies stable". Silently
/// degrading on a missing nightly hides a real config issue.
///
/// Where: called at the top of `rustdoc_overlay` before any I/O.
fn ensure_nightly_toolchain() -> std::result::Result<(), Error> {
    let result = process::Command::new("cargo")
        .args(["+nightly", "--version"])
        .output();
    match result {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(Error::ToolchainMissing {
            reason: format!(
                "cargo +nightly --version exited {}: {}",
                o.status,
                String::from_utf8_lossy(&o.stderr).trim()
            ),
        }),
        Err(e) => Err(Error::ToolchainMissing {
            reason: format!("cargo invocation failed: {}", e),
        }),
    }
}

/// What: resolve the package name to pass via `-p` to rustdoc. Mirrors
/// rustdoc_overlay.py's `_resolve_package` priority order: caller-
/// supplied match -> fingerprint's most-depended-on -> repo-name
/// match (with lib<name> + hyphen/underscore equivalents) -> first
/// workspace lib target -> caller's value verbatim.
///
/// Why: the agent typically passes the repo name, which may not be a
/// workspace member (sourcetrait_common is the canonical example).
/// Reaching into cargo metadata + the characterize fingerprint
/// surfaces the canonical workspace member.
///
/// Where: called by `rustdoc_overlay` to compute the `-p` argument
/// before invoking rustdoc.
fn resolve_package(
    root: &Path,
    requested: Option<&str>,
    orientation_dir: &Path,
) -> Option<String> {
    let meta = match cargo_metadata(root) {
        Some(m) => m,
        None => return requested.map(String::from),
    };
    let packages: indexmap::IndexMap<String, serde_json::Value> = meta
        .get("packages")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| {
            p.get("name")
                .and_then(|n| n.as_str().map(|s| (s.to_string(), p.clone())))
        })
        .collect();
    if packages.is_empty() {
        return requested.map(String::from);
    }

    if let Some(r) = requested {
        if packages.contains_key(r) {
            return Some(r.to_string());
        }
    }

    let fp_path = orientation_dir.join("fingerprint.json");
    if fp_path.exists() {
        if let Ok(fp_text) = fs::read_to_string(&fp_path) {
            if let Ok(fp) = serde_json::from_str::<serde_json::Value>(&fp_text) {
                let per_crate = fp
                    .get("per_crate")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();
                // Scaffolding crates (example / demo members) must not
                // steer the package pick: ratatui's ~36 example crates
                // all depend on the FACADE crate, electing it over
                // ratatui-core - and a facade's rustdoc index is all
                // re-exports (0 impls), which produced the 0/0 overlay
                // outlier. Count dependent votes from substantive
                // crates only.
                let scaffolding: std::collections::HashSet<String> = fp
                    .get("workspace_use_classification")
                    .and_then(|v| v.get("scaffolding_crates"))
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let mut dep_count: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
                for (voter, c) in per_crate.iter() {
                    if scaffolding.contains(voter) {
                        continue;
                    }
                    if let Some(deps) = c.get("deps").and_then(|v| v.as_array()) {
                        for d in deps {
                            if let Some(s) = d.as_str() {
                                *dep_count.entry(s.to_string()).or_insert(0) += 1;
                            }
                        }
                    }
                }
                let in_ws: indexmap::IndexMap<String, usize> = dep_count
                    .iter()
                    .filter(|(k, _)| packages.contains_key(*k))
                    .map(|(k, v)| (k.clone(), *v))
                    .collect();
                if !in_ws.is_empty() {
                    let mut best: Option<(String, usize)> = None;
                    for (k, v) in &in_ws {
                        best = Some(match best {
                            None => (k.clone(), *v),
                            Some((_, bv)) if *v > bv => (k.clone(), *v),
                            Some(prev) => prev,
                        });
                    }
                    if let Some((p, _)) = best {
                        return Some(p);
                    }
                }
            }
        }
    }

    let repo_name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    if packages.contains_key(&repo_name) {
        return Some(repo_name);
    }
    let normalized = repo_name.replace('-', "_");
    for p_name in packages.keys() {
        if p_name == &format!("lib{}", repo_name) || p_name.replace('-', "_") == normalized {
            return Some(p_name.clone());
        }
    }
    for (p_name, p_data) in &packages {
        if let Some(targets) = p_data.get("targets").and_then(|v| v.as_array()) {
            for t in targets {
                if let Some(kinds) = t.get("kind").and_then(|v| v.as_array()) {
                    if kinds.iter().any(|k| k.as_str() == Some("lib")) {
                        return Some(p_name.clone());
                    }
                }
            }
        }
    }
    requested.map(String::from)
}

/// What: run `cargo +nightly metadata --no-deps --format-version 1`
/// at the given root and parse the JSON. Returns None on failure (the
/// caller falls back to the requested package).
///
/// Why: cosmic-epoch's submodule-aggregation layout has no top-level
/// Cargo.toml; cargo metadata fails there. Quiet None on failure lets
/// the caller try other paths.
///
/// Where: called by `resolve_package`.
fn cargo_metadata(root: &Path) -> Option<serde_json::Value> {
    let result = process::Command::new("cargo")
        .args(["+nightly", "metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .output()
        .ok()?;
    if !result.status.success() {
        return None;
    }
    serde_json::from_slice::<serde_json::Value>(&result.stdout).ok()
}

/// What: invoke `cargo +nightly rustdoc -p <pkg> --lib -- -Z
/// unstable-options --output-format json` at the project root and
/// read the resulting target/doc/<crate>.json. Retries with `--bin`
/// when rustdoc errors with "no library targets" (cosmic-comp-style
/// bin-only crates).
///
/// Why: rustdoc_overlay.py's `run_rustdoc_json` ported to Rust. The
/// --cap-lints allow override matches the Python tool so strict-doc
/// projects (iced ships with `-F rustdoc::broken-intra-doc-links`)
/// don't block the JSON build.
///
/// Where: called by `rustdoc_overlay` after `resolve_package`.
fn run_rustdoc_json(
    root: &Path,
    package: Option<&str>,
) -> std::result::Result<serde_json::Value, Error> {
    let extra = ["--", "-Z", "unstable-options", "--output-format", "json"];
    let mut cmd_args: Vec<String> = vec!["+nightly".into(), "rustdoc".into()];
    if let Some(p) = package {
        cmd_args.push("-p".into());
        cmd_args.push(p.to_string());
    }
    let mut first_cmd = cmd_args.clone();
    first_cmd.push("--lib".to_string());
    for e in &extra {
        first_cmd.push((*e).to_string());
    }
    let first_run = process::Command::new("cargo")
        .args(&first_cmd)
        .current_dir(root)
        .env("RUSTDOCFLAGS", "--cap-lints allow")
        .output()
        .map_err(|source| Error::RustdocFailed {
            reason: format!("cargo invocation failed: {}", source),
        })?;
    let mut final_result = first_run;
    if !final_result.status.success() {
        let stderr_text = String::from_utf8_lossy(&final_result.stderr);
        if package.is_some()
            && (stderr_text.contains("no library targets")
                || stderr_text.contains("no lib target"))
        {
            let mut bin_cmd = cmd_args.clone();
            bin_cmd.push("--bin".to_string());
            bin_cmd.push(package.unwrap().to_string());
            for e in &extra {
                bin_cmd.push((*e).to_string());
            }
            final_result = process::Command::new("cargo")
                .args(&bin_cmd)
                .current_dir(root)
                .env("RUSTDOCFLAGS", "--cap-lints allow")
                .output()
                .map_err(|source| Error::RustdocFailed {
                    reason: format!("cargo invocation failed: {}", source),
                })?;
        }
    }
    if !final_result.status.success() {
        return Err(Error::RustdocFailed {
            reason: format!(
                "cargo +nightly rustdoc exited {}: {}",
                final_result.status,
                String::from_utf8_lossy(&final_result.stderr).trim()
            ),
        });
    }
    let docdir = root.join("target").join("doc");
    let mut candidates: Vec<PathBuf> = fs::read_dir(&docdir)
        .map_err(|source| Error::Read {
            path: docdir.clone(),
            source,
        })?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    candidates.sort();
    // Prefer the JSON matching the built package (target/doc keeps
    // stale JSONs from earlier runs; alphabetically-last is wrong
    // whenever another crate's doc lingers).
    let by_name = package.and_then(|p| {
        let want = format!("{}.json", p.replace('-', "_"));
        candidates
            .iter()
            .find(|c| {
                c.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n == want)
                    .unwrap_or(false)
            })
            .cloned()
    });
    let latest = by_name
        .or_else(|| candidates.last().cloned())
        .ok_or_else(|| Error::Read {
            path: docdir.clone(),
            source: io::Error::new(io::ErrorKind::NotFound, "no rustdoc JSON produced"),
        })?;
    let body = fs::read_to_string(&latest).map_err(|source| Error::Read {
        path: latest.clone(),
        source,
    })?;
    serde_json::from_str(&body).map_err(|source| Error::Serialize { source })
}

/// What: build the overlay from floor facts.json + rustdoc JSON.
/// Hard-errors on unknown format_version per locked decision 4 (Rust
/// port diverges from Python's degrade-banner path).
///
/// Why: rustdoc_overlay.py's `reconcile` (lines 165-227). Splits
/// rustdoc-only impls into std-blanket-coverage vs user-domain macro
/// generation; flags null-span items; collects re-export sources;
/// produces the disagreement signal that drives S3 seam-spine
/// narrative.
///
/// Where: called by `rustdoc_overlay` after `run_rustdoc_json`.
fn reconcile(
    floor_facts: &serde_json::Value,
    rustdoc: &serde_json::Value,
) -> std::result::Result<Overlay, Error> {
    let fv = rustdoc
        .get("format_version")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| Error::UnknownRustdocFormatVersion {
            version: "missing".to_string(),
        })?;
    if !(FORMAT_VERSION_MIN..=FORMAT_VERSION_MAX).contains(&fv) {
        return Err(Error::UnknownRustdocFormatVersion {
            version: fv.to_string(),
        });
    }
    let empty_map = serde_json::Map::new();
    let index = rustdoc
        .get("index")
        .and_then(|v| v.as_object())
        .unwrap_or(&empty_map);

    let mut macro_generated: Vec<MacroGenerated> = Vec::new();
    let mut reexports: Vec<Reexport> = Vec::new();
    let mut null_span_items: Vec<NullSpanItem> = Vec::new();

    let empty_array: Vec<serde_json::Value> = Vec::new();
    let floor_impls = floor_facts
        .get("impls")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_array);
    let mut floor_impl_traits: HashSet<String> = HashSet::new();
    for i in floor_impls {
        if let Some(t) = i.get("trait").and_then(|v| v.as_str()) {
            floor_impl_traits.insert(t.to_string());
        }
    }

    for item in index.values() {
        let name = item.get("name").and_then(|v| v.as_str()).map(String::from);
        let span_present = !item.get("span").map(|v| v.is_null()).unwrap_or(true);
        let inner = item.get("inner");
        let kind = inner
            .and_then(|v| v.as_object())
            .and_then(|o| o.keys().next().cloned());
        if !span_present {
            if let Some(n) = &name {
                null_span_items.push(NullSpanItem {
                    name: n.clone(),
                    kind: kind.clone(),
                });
            }
        }
        if kind.as_deref() == Some("impl") {
            if let Some(impl_inner) = inner.and_then(|v| v.get("impl")) {
                let tr_obj = impl_inner.get("trait");
                let tr_name: Option<String> = tr_obj
                    .and_then(|v| v.get("path").and_then(|p| p.as_str()))
                    .or_else(|| tr_obj.and_then(|v| v.get("name").and_then(|n| n.as_str())))
                    .map(String::from);
                if !span_present {
                    if let Some(t) = tr_name {
                        macro_generated.push(MacroGenerated {
                            trait_name: t,
                            span: None,
                            note: "no span (blanket/synth/macro)",
                        });
                    }
                }
            }
        }
        if matches!(kind.as_deref(), Some("use") | Some("import")) {
            let kind_key = kind.as_deref().unwrap_or("");
            let tgt = inner.and_then(|v| v.get(kind_key));
            let source = tgt
                .and_then(|v| v.as_object())
                .and_then(|o| o.get("source").cloned());
            reexports.push(Reexport { name, source });
        }
    }

    let rustdoc_impl_traits: HashSet<String> = macro_generated
        .iter()
        .map(|m| m.trait_name.clone())
        .collect();
    let mut only_rustdoc: Vec<String> = rustdoc_impl_traits
        .difference(&floor_impl_traits)
        .cloned()
        .collect();
    only_rustdoc.sort();
    let std_blanket_set: HashSet<&str> = STD_BLANKET_TRAITS.iter().copied().collect();
    let std_blanket: Vec<String> = only_rustdoc
        .iter()
        .filter(|t| std_blanket_set.contains(t.as_str()))
        .cloned()
        .collect();
    let user_macro: Vec<String> = only_rustdoc
        .iter()
        .filter(|t| !std_blanket_set.contains(t.as_str()))
        .cloned()
        .collect();
    let mut disagreements: Vec<Disagreement> = Vec::new();
    if !std_blanket.is_empty() {
        disagreements.push(Disagreement {
            kind: "std_blanket_coverage",
            traits: std_blanket,
            note: "Standard-library blanket / auto-impl coverage (Any/Send/Sync/From/Into/etc.) that the source-text scanner cannot see. Compiler-synthesized; NOT a user-macro-registration seam. Informational.",
        });
    }
    if !user_macro.is_empty() {
        disagreements.push(Disagreement {
            kind: "impls_only_in_rustdoc",
            traits: user_macro,
            note: "rustdoc sees impls the source scanner did not -- likely user-domain macro-generated. Confirms a macro-registration seam (derive macros, attribute macros, or registration!() expansions). Verify by reading the macro invocation sites in reference.md.",
        });
    }
    Ok(Overlay {
        status: "ok",
        format_version: fv,
        macro_generated,
        reexports,
        null_span_items,
        disagreements,
    })
}
