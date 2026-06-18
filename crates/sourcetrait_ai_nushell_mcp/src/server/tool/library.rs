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

/// One diagnostic row in a `check` result (an error or a warning). `kind` is
/// namespaced (`structure::...` / `lint::...`); `position` is `[line, col]`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct CheckDiagnostic {
    pub kind: String,
    pub path: String,
    pub position: Vec<usize>,
    pub message: String,
}

/// `check` summary: the library's `cargo test`. Errors block a commit;
/// warnings are advisory. `num_errors` / `num_warnings` are the authoritative
/// totals (the row vectors may be capped by the validator).
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct CheckSummary {
    pub ok: bool,
    pub num_errors: usize,
    pub num_warnings: usize,
    pub errors: Vec<CheckDiagnostic>,
    pub warnings: Vec<CheckDiagnostic>,
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
    async fn library(
        &self,
        mcp::Parameters(p): mcp::Parameters<LibraryParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let source_dir = std::path::Path::new(&p.source_dir);
        match p.action.as_str() {
            // Establish a fresh, empty library at source_dir + register it.
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
            // Bring a shipped/complete source into the mcp: establish + first
            // commit, atomic.
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
            // Validate the in-source tree (cargo-test equivalent); no mutation.
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
            // Remove from the mcp; source untouched. Idempotent (absent == ok).
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

/// Bucket a `ValidationResult` into the `check` summary: structural violations
/// -> errors, the doc lint -> warnings. (Fine-grained per-emit `structure::*`
/// kinds land with the diagnostic-unification leg; for now structural rows
/// carry a coarse `structure::violation` kind. num_* are authoritative totals.)
fn check_summary_from(result: &ValidationResult) -> CheckSummary {
    let errors: Vec<CheckDiagnostic> = result
        .structural
        .iter()
        .map(|v| CheckDiagnostic {
            kind: "structure::violation".to_string(),
            path: v.path.clone(),
            position: vec![v.line, 0],
            message: v.message.clone(),
        })
        .collect();
    let warnings: Vec<CheckDiagnostic> =
        result.lint.iter().filter_map(lint_to_diagnostic).collect();
    CheckSummary {
        ok: errors.is_empty(),
        num_errors: errors.len(),
        num_warnings: warnings.len(),
        errors,
        warnings,
    }
}

/// Map a `LintViolation` to a `check` warning row with its namespaced kind +
/// a short message. The `More` truncation sentinel maps to None (the totals
/// already convey truncation).
fn lint_to_diagnostic(v: &LintViolation) -> Option<CheckDiagnostic> {
    let (kind, position, source, message): (&str, [usize; 2], &Option<WhereSource>, &str) = match v {
        LintViolation::SummaryLength { position, source } => (
            "lint::summary_length",
            *position,
            source,
            "doc summary line exceeds 80 characters",
        ),
        LintViolation::HardcodedVariable { position, source } => (
            "lint::hardcoded_variable",
            *position,
            source,
            "hardcoded path literal; lift it to args",
        ),
        LintViolation::DeniedCommand { position, source } => (
            "lint::denied_command",
            *position,
            source,
            "denied external command; use a nushell builtin",
        ),
        LintViolation::More => return None,
    };
    let path = match source {
        Some(WhereSource::Mod(p)) => p.clone(),
        _ => String::new(),
    };
    Some(CheckDiagnostic {
        kind: kind.to_string(),
        path,
        position: position.to_vec(),
        message: message.to_string(),
    })
}
