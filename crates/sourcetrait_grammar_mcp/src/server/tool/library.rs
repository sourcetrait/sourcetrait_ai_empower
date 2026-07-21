use crate::*;

/// Parameters for `library()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct LibraryParams {
    /// One of: `new`, `install`, `check`, `uninstall`.
    pub action: String,
    /// Library name (a snake identifier).
    pub library: String,
    /// The library's source directory. The "are you sure" cross-check on
    /// every action: for an already-registered library it must equal the
    /// recorded source_path; for `new` / `install` it is the path recorded.
    pub source_dir: String,
}

/// `new` summary: the paths scaffolded for the fresh (empty) library.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct NewSummary {
    pub created: Vec<String>,
}

/// `install` summary: the first commit's changed paths, grouped.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct InstallSummary {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub removed: Vec<String>,
}

/// `check` summary: the library's validation pass. Error-severity rows block a
/// commit; Warning-severity rows advise. The rows are the unified
/// `Diagnostic`s bucketed by severity; `ok` is true iff there are no errors.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct CheckSummary {
    pub ok: bool,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

/// The per-action result, serialized as `{ summary: oneof<...> }`. `uninstall`
/// returns no summary (void; idempotent success). Untagged: the caller knows
/// the variant from the action it invoked, and the shapes are distinct.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
#[serde(untagged)]
pub(crate) enum LibrarySummary {
    New(NewSummary),
    Install(InstallSummary),
    Check(CheckSummary),
}

/// Success envelope for `library()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct LibraryEnvelope {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<LibrarySummary>,
}

#[mcp::tool_router(router = library_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Library administration: new, install, check, uninstall.",
        output_schema = mcp::schema_for_type::<LibraryEnvelope>()
    )]
    pub(crate) async fn library(
        &self,
        mcp::Parameters(p): mcp::Parameters<LibraryParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let source_dir = std::path::Path::new(&p.source_dir);
        match p.action.as_str() {
            "new" => {
                let lock = match self.library_locks.register(&p.library).await {
                    Ok(l) => l,
                    Err(_) => {
                        return Ok(error_to_call_result(
                            Error::LibraryAlreadyRegistered {
                                library: p.library.clone(),
                            },
                            None,
                        ));
                    }
                };
                let _guard = lock.write().await;
                match establish_library(&p.library, source_dir) {
                    Ok(()) => {
                        let created =
                            vec![source_dir.join("mod.nu").to_string_lossy().into_owned()];
                        envelope_to_structured(&LibraryEnvelope {
                            summary: Some(LibrarySummary::New(NewSummary { created })),
                        })
                    }
                    Err(e) => {
                        self.library_locks.unregister(&p.library).await;
                        Ok(error_to_call_result(e, None))
                    }
                }
            }
            "install" => {
                let lock = match self.library_locks.register(&p.library).await {
                    Ok(l) => l,
                    Err(_) => {
                        return Ok(error_to_call_result(
                            Error::LibraryAlreadyRegistered {
                                library: p.library.clone(),
                            },
                            None,
                        ));
                    }
                };
                let _guard = lock.write().await;
                let engine = self.lint_engine.current();
                match install_impl(&p.library, source_dir, &engine) {
                    Ok(result) => {
                        purview_add_library(&p.library, &self.current_purview.ids());
                        envelope_to_structured(&LibraryEnvelope {
                            summary: Some(LibrarySummary::Install(InstallSummary {
                                added: result.added,
                                modified: result.modified,
                                removed: result.removed,
                            })),
                        })
                    }
                    Err(e) => {
                        self.library_locks.unregister(&p.library).await;
                        Ok(error_to_call_result(e, None))
                    }
                }
            }
            "check" => {
                let lock = match self.library_locks.lookup(&p.library).await {
                    Some(l) => l,
                    None => {
                        return Ok(error_to_call_result(
                            Error::LibraryNotRegistered {
                                library: p.library.clone(),
                            },
                            None,
                        ));
                    }
                };
                let _guard = lock.read().await;
                if let Err(e) = check_source_dir(&p.library, &p.source_dir) {
                    return Ok(error_to_call_result(e, None));
                }
                let engine = self.lint_engine.current();
                match check_library(&p.library, &engine) {
                    Ok(result) => envelope_to_structured(&LibraryEnvelope {
                        summary: Some(LibrarySummary::Check(check_summary_from(&result))),
                    }),
                    Err(e) => Ok(error_to_call_result(e, None)),
                }
            }
            "uninstall" => {
                let lock = match self.library_locks.lookup(&p.library).await {
                    Some(l) => l,
                    None => {
                        return envelope_to_structured(&LibraryEnvelope { summary: None });
                    }
                };
                let _guard = lock.write().await;
                if let Err(e) = check_source_dir(&p.library, &p.source_dir) {
                    return Ok(error_to_call_result(e, None));
                }
                match uninstall_impl(&p.library) {
                    Ok(()) => {
                        drop(_guard);
                        self.library_locks.unregister(&p.library).await;
                        purview_remove_library(&p.library);
                        envelope_to_structured(&LibraryEnvelope { summary: None })
                    }
                    Err(e) => Ok(error_to_call_result(e, None)),
                }
            }
            other => Ok(error_to_call_result(
                Error::LibraryInvalidAction {
                    action: other.to_string(),
                },
                None,
            )),
        }
    }
}

/// Bring a freshly installed rig into view.
///
/// ADDS, never replaces. An UNCONFIGURED purview is implicitly EVERYTHING, so
/// its `*` is materialized FIRST and the new pattern appended beside it - the
/// first install on a fresh store therefore writes `['*', 'my/lib:']` and
/// nothing leaves view. Narrowing the whole store down to one library as the
/// price of installing it would be a surprising trade.
///
/// The CURRENT view is a set of purview ids rather than of patterns, so "add it
/// to the current purview" means adding it to each configured purview that is
/// currently in view - `default` always, plus whatever else the session named.
///
/// Non-fatal: the install already succeeded and is not rolled back, so a
/// bookkeeping failure is reported to stderr rather than turned into a failed
/// install. It is worth reading, because a configured `default` that failed to
/// gain the pattern leaves the new library installed but out of view.
fn purview_add_library(
    library: &str,
    current_ids: &[String],
) {
    let pattern = format!("{library}:");
    let mut rows = match load_purviews() {
        Ok(rows) => rows.unwrap_or_default(),
        Err(e) => {
            eprintln!("grammar: purview not updated for {library}: {e:?}");
            return;
        }
    };
    let mut targets: Vec<String> = vec![PURVIEW_DEFAULT.to_string()];
    for id in current_ids {
        if id != PURVIEW_ALL && !targets.contains(id) {
            targets.push(id.clone());
        }
    }
    for id in targets {
        match rows.iter_mut().find(|row| row.id == id) {
            Some(row) => {
                if !row.namepath_patterns.contains(&pattern) {
                    row.namepath_patterns.push(pattern.clone());
                }
            }
            None => rows.push(PurviewRow {
                id,
                namepath_patterns: vec![PURVIEW_ALL.to_string(), pattern.clone()],
            }),
        }
    }
    prune_dangling(&mut rows);
    if let Err(e) = save_purviews(&rows) {
        eprintln!("grammar: purview not updated for {library}: {e:?}");
    }
}

/// Drop an uninstalled rig from every purview that named it.
///
/// The bare `author/name:` pattern goes explicitly; anything ELSE that pointed
/// into the library - a module pattern beneath it, an exact call inside it - is
/// now dangling and goes with the prune, which is the same sweep any other
/// detection runs.
///
/// An UNCONFIGURED namespace has nothing to update, and must not be
/// materialized here: writing a file on uninstall would silently convert
/// "everything is in view" into a configuration nobody asked for.
fn purview_remove_library(library: &str) {
    let rows = match load_purviews() {
        Ok(Some(rows)) => Some(rows),
        Ok(None) => None,
        Err(e) => {
            eprintln!("grammar: purview not updated for {library}: {e:?}");
            return;
        }
    };
    let Some(mut rows) = rows else {
        return;
    };
    let pattern = format!("{library}:");
    for row in rows.iter_mut() {
        row.namepath_patterns.retain(|selector| selector != &pattern);
    }
    prune_dangling(&mut rows);
    if let Err(e) = save_purviews(&rows) {
        eprintln!("grammar: purview not updated for {library}: {e:?}");
    }
}

fn check_summary_from(result: &ValidationResult) -> CheckSummary {
    let (errors, warnings) = Diagnostic::bucket(result.diagnostics.clone());
    CheckSummary {
        ok: errors.is_empty(),
        errors,
        warnings,
    }
}
