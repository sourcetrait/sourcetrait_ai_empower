use crate::*;

/// Parameters for `rig()`.
#[derive(Debug, ser::Deserialize, ser::Serialize, schema::JsonSchema)]
pub struct RigParams {
    /// One of: `new`, `install`, `check`, `uninstall`.
    pub action: String,
    /// The rig: the compound `<author>/<name>`.
    pub rig: String,
    /// The rig's source directory: the "are you sure" cross-check.
    pub source_dir: String,
}

/// `new` summary: the paths scaffolded for the fresh (empty) rig.
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

/// `check` summary: `ok` is true iff there are no error rows.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct CheckSummary {
    pub ok: bool,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

/// The per-action result; `uninstall` returns no summary.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
#[serde(untagged)]
pub(crate) enum RigSummary {
    New(NewSummary),
    Install(InstallSummary),
    Check(CheckSummary),
}

/// Success envelope for `rig()`.
#[derive(Debug, ser::Serialize, schema::JsonSchema)]
pub(crate) struct RigEnvelope {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<RigSummary>,
}

#[mcp::tool_router(router = rig_router, vis = "pub(crate)")]
impl NuSh {
    #[mcp::tool(
        description = "Rig administration: new, install, check, uninstall.",
        output_schema = mcp::schema_for_type::<RigEnvelope>()
    )]
    pub(crate) async fn rig(
        &self,
        mcp::Parameters(p): mcp::Parameters<RigParams>,
    ) -> Result<mcp::CallToolResult, mcp::ErrorData> {
        let source_dir = std::path::Path::new(&p.source_dir);
        match p.action.as_str() {
            "new" => {
                let lock = match self.rig_locks.register(&p.rig).await {
                    Ok(l) => l,
                    Err(_) => {
                        return Ok(error_to_call_result(
                            GrammarMcpError::RigAlreadyRegistered {
                                rig: p.rig.clone(),
                            },
                            None,
                        ));
                    }
                };
                let _guard = lock.write().await;
                match establish_rig(&p.rig, source_dir) {
                    Ok(()) => {
                        let created =
                            vec![source_dir.join("mod.nu").to_string_lossy().into_owned()];
                        envelope_to_structured(&RigEnvelope {
                            summary: Some(RigSummary::New(NewSummary { created })),
                        })
                    }
                    Err(e) => {
                        self.rig_locks.unregister(&p.rig).await;
                        Ok(error_to_call_result(e, None))
                    }
                }
            }
            "install" => {
                let lock = match self.rig_locks.register(&p.rig).await {
                    Ok(l) => l,
                    Err(_) => {
                        return Ok(error_to_call_result(
                            GrammarMcpError::RigAlreadyRegistered {
                                rig: p.rig.clone(),
                            },
                            None,
                        ));
                    }
                };
                let _guard = lock.write().await;
                let engine = self.lint_engine.current();
                match install_impl(&p.rig, source_dir, &engine) {
                    Ok(result) => {
                        purview_add_rig(&p.rig, &self.current_purview.ids());
                        envelope_to_structured(&RigEnvelope {
                            summary: Some(RigSummary::Install(InstallSummary {
                                added: result.added,
                                modified: result.modified,
                                removed: result.removed,
                            })),
                        })
                    }
                    Err(e) => {
                        self.rig_locks.unregister(&p.rig).await;
                        Ok(error_to_call_result(e, None))
                    }
                }
            }
            "check" => {
                let lock = match self.rig_locks.lookup(&p.rig).await {
                    Some(l) => l,
                    None => {
                        return Ok(error_to_call_result(
                            GrammarMcpError::RigNotRegistered {
                                rig: p.rig.clone(),
                            },
                            None,
                        ));
                    }
                };
                let _guard = lock.read().await;
                if let Err(e) = check_source_dir(&p.rig, &p.source_dir) {
                    return Ok(error_to_call_result(e, None));
                }
                let engine = self.lint_engine.current();
                match check_rig(&p.rig, &engine) {
                    Ok(result) => envelope_to_structured(&RigEnvelope {
                        summary: Some(RigSummary::Check(check_summary_from(&result))),
                    }),
                    Err(e) => Ok(error_to_call_result(e, None)),
                }
            }
            "uninstall" => {
                let lock = match self.rig_locks.lookup(&p.rig).await {
                    Some(l) => l,
                    None => {
                        return envelope_to_structured(&RigEnvelope { summary: None });
                    }
                };
                let _guard = lock.write().await;
                if let Err(e) = check_source_dir(&p.rig, &p.source_dir) {
                    return Ok(error_to_call_result(e, None));
                }
                match uninstall_impl(&p.rig) {
                    Ok(()) => {
                        drop(_guard);
                        self.rig_locks.unregister(&p.rig).await;
                        purview_remove_rig(&p.rig);
                        envelope_to_structured(&RigEnvelope { summary: None })
                    }
                    Err(e) => Ok(error_to_call_result(e, None)),
                }
            }
            other => Ok(error_to_call_result(
                GrammarMcpError::RigInvalidAction {
                    action: other.to_string(),
                },
                None,
            )),
        }
    }
}

/// Bring a freshly installed rig into view.
fn purview_add_rig(
    rig: &str,
    current_ids: &[String],
) {
    // The purview table is namespace meta beside the rigs repo: its
    // load-mutate-save is not atomic, so it rides the same repo-wide lock.
    let _repo = rigs_repo_lock();
    let pattern = format!("{rig}:");
    let mut rows = match load_purviews() {
        Ok(rows) => rows.unwrap_or_default(),
        Err(e) => {
            eprintln!("grammar: purview not updated for {rig}: {e:?}");
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
        if let Some(row) = rows.iter_mut().find(|row| row.id == id) {
            if !row.namepath_patterns.contains(&pattern) {
                row.namepath_patterns.push(pattern.clone());
            }
        }
    }
    prune_dangling(&mut rows);
    if let Err(e) = save_purviews(&rows) {
        eprintln!("grammar: purview not updated for {rig}: {e:?}");
    }
}

/// Drop an uninstalled rig from every purview that named it.
fn purview_remove_rig(rig: &str) {
    let _repo = rigs_repo_lock();
    let mut rows = match load_purviews() {
        Ok(rows) => rows.unwrap_or_default(),
        Err(e) => {
            eprintln!("grammar: purview not updated for {rig}: {e:?}");
            return;
        }
    };
    let pattern = format!("{rig}:");
    for row in rows.iter_mut() {
        row.namepath_patterns.retain(|p| p != &pattern);
    }
    prune_dangling(&mut rows);
    if let Err(e) = save_purviews(&rows) {
        eprintln!("grammar: purview not updated for {rig}: {e:?}");
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
