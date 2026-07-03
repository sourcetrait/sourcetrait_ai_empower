use crate::*;

/// What: the one-shot CLI driver. Builds the same substrate the MCP
/// serve path uses (keypair + libraries repo + lock registry, the lazy
/// stateless pool, a lazy interact slot), dispatches ONE tool invocation
/// directly to the pub(crate) tool handlers, prints the envelope as bare
/// compact JSON (one line, machine format -- the cli's consumer is a
/// wrapper, never a human eye) on stdout, and exits: 0 on success, 1 on
/// an error envelope or rmcp-boundary error, 2 on unparseable operator
/// input.
///
/// Why: the operator gets the identical tool surface with no agent and no
/// MCP client -- same impls, same envelopes, no rmcp transport. Deny does
/// not apply (it gates agent registration; this surface is the
/// operator's). interact is single-shot (session state dies with this
/// process); processes/kill are process-scoped, so a one-shot invocation
/// shows none. Writes into a store a live agent host is using are the
/// operator's own risk (git's index lock keeps the repo itself safe).
///
/// Where: called by `cli::host_main` for the `cli` subcommand; CONFIG is
/// already stored.
pub(crate) fn run_oneshot(tool: CliTool) {
    let rt = tk::Runtime::new().expect("tokio Runtime::new");
    let exit_code = rt.block_on(async move {
        let library_locks = ensure_substrate().await.expect("ensure_substrate");
        let runs_pool = Pool::new(
            Mode::Stateless,
            worker_pool_cap(),
            1,
            tk::TkDuration::from_secs(60),
        );
        let nonce_gen = Arc::new(NonceGen::new());
        let lint_engine = Arc::new(ParseEngine::new_full());
        // interact slot starts None: an info/inspect one-shot never pays a
        // stateful-worker spawn; `cli interact` lazy-spawns via
        // dispatch_interact.
        let server = NuSh::new(runs_pool, None, nonce_gen, library_locks, lint_engine);
        let result = match tool {
            CliTool::Info => server.info(mcp::Parameters(InfoParams {})).await,
            CliTool::Inspect { namepath } => {
                server
                    .inspect(mcp::Parameters(InspectParams { namepath }))
                    .await
            }
            CliTool::Call {
                namepath,
                args,
                timeout_ms,
            } => {
                server
                    .call(mcp::Parameters(CallParams {
                        namepath,
                        args: nuon_record_arg(args.as_deref()),
                        timeout_ms,
                    }))
                    .await
            }
            CliTool::Run {
                body,
                args_schema,
                result_schema,
                args,
                timeout_ms,
            } => {
                server
                    .run(mcp::Parameters(RunParams {
                        args_schema: nuon_record_arg(args_schema.as_deref()),
                        result_schema: nuon_record_arg(result_schema.as_deref()),
                        args: nuon_record_arg(args.as_deref()),
                        body,
                        timeout_ms,
                    }))
                    .await
            }
            CliTool::Interact {
                body,
                args_schema,
                result_schema,
                args,
                timeout_ms,
            } => {
                server
                    .interact(mcp::Parameters(RunParams {
                        args_schema: nuon_record_arg(args_schema.as_deref()),
                        result_schema: nuon_record_arg(result_schema.as_deref()),
                        args: nuon_record_arg(args.as_deref()),
                        body,
                        timeout_ms,
                    }))
                    .await
            }
            CliTool::Rerun {
                rerun_id,
                args,
                timeout_ms,
            } => {
                server
                    .rerun(mcp::Parameters(RerunParams {
                        rerun_id,
                        args: nuon_record_arg(args.as_deref()),
                        timeout_ms,
                    }))
                    .await
            }
            CliTool::Processes => {
                server
                    .processes(mcp::Parameters(ProcessesParams {}))
                    .await
            }
            CliTool::Kill { nonce } => server.kill(mcp::Parameters(KillParams { nonce })).await,
            CliTool::Learn { harness_dir } => {
                server
                    .learn(mcp::Parameters(LearnParams { harness_dir }))
                    .await
            }
            CliTool::New { namepaths } => {
                server
                    .scaffold(mcp::Parameters(NewParams { namepaths }))
                    .await
            }
            CliTool::Commit { library } => {
                server
                    .commit(mcp::Parameters(CommitParams { library }))
                    .await
            }
            CliTool::Library {
                action,
                library,
                source_dir,
            } => {
                server
                    .library(mcp::Parameters(LibraryParams {
                        action: action.as_str().to_string(),
                        library,
                        source_dir,
                    }))
                    .await
            }
        };
        match result {
            Ok(r) => match r.structured_content {
                Some(value) => {
                    // The `error` wrapper is the success-vs-error
                    // discriminator on every envelope.
                    let failed = value.get("error").is_some();
                    // serde_json::Value's Display IS compact JSON -- the
                    // machine format, one line, no color, no indentation.
                    println!("{value}");
                    if failed { 1 } else { 0 }
                }
                // No-return success (kill): absence == success, print
                // nothing.
                None => 0,
            },
            Err(e) => {
                eprintln!("{e}");
                1
            }
        }
    });
    // Drop the runtime BEFORE exiting so the pool + any workers tear down
    // (WorkerHandle::drop SIGKILLs its child).
    drop(rt);
    process::exit(exit_code);
}

/// What: parse an optional single-quoted NUON record argument (e.g.
/// '{x: 5}') into the `mcp::JsonObject` the tool params carry. Absent or
/// blank input is the empty record. Exits 2 on unparseable input or a
/// non-record value (operator input error, before any tool dispatch).
///
/// Why: NUON is the operator's native literal syntax at a nushell
/// terminal; the tool params carry JSON objects. nu_json::Value is the
/// same converter the worker returns results through, so the round-trip
/// semantics match `to json`.
fn nuon_record_arg(input: Option<&str>) -> mcp::JsonObject {
    let Some(raw) = input else {
        return mcp::JsonObject::new();
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return mcp::JsonObject::new();
    }
    let value = match nu::from_nuon(trimmed, None) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("invalid NUON record `{trimmed}`: {e}");
            process::exit(2);
        }
    };
    let json_compat = match nu::JsonValue::from_value(value) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("NUON value does not convert to JSON: {e}");
            process::exit(2);
        }
    };
    let serde_value = match json::to_value(&json_compat) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("NUON value does not serialize: {e}");
            process::exit(2);
        }
    };
    match serde_value {
        json::Value::Object(map) => map,
        other => {
            eprintln!("expected a NUON record (e.g. '{{x: 5}}'), got `{other}`");
            process::exit(2);
        }
    }
}
