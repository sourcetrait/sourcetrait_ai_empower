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

/// `check` summary: the library's `cargo test`. Error-severity rows block a
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
                match install_impl(&p.library, source_dir, &self.lint_engine) {
                    Ok(result) => envelope_to_structured(&LibraryEnvelope {
                        summary: Some(LibrarySummary::Install(InstallSummary {
                            added: result.added,
                            modified: result.modified,
                            removed: result.removed,
                        })),
                    }),
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
                match check_library(&p.library, &self.lint_engine) {
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

fn check_summary_from(result: &ValidationResult) -> CheckSummary {
    let (errors, warnings) = Diagnostic::bucket(result.diagnostics.clone());
    CheckSummary {
        ok: errors.is_empty(),
        errors,
        warnings,
    }
}
