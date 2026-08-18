use crate::*;

pub(crate) async fn run_oneshot(tool: CliTool) -> process::ExitCode {
    let exit_code = async move {
        install_child_subreaper();
        let rig_locks = ensure_substrate().await.expect("ensure_substrate");
        let nonce_gen = Arc::new(datum::NonceGenerator::new());
        let lint_engine = Arc::new(LintEngine::new());
        let server = NuSh::new(nonce_gen, rig_locks, lint_engine);
        let result = match tool {
            CliTool::Info => {
                server
                    .info(mcp::Parameters(InfoParams {
                        purviews: Vec::new(),
                    }))
                    .await
            }
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
                nonce,
                args,
                timeout_ms,
            } => {
                server
                    .rerun(mcp::Parameters(RerunParams {
                        nonce,
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
            CliTool::Commit { rig } => {
                server
                    .commit(mcp::Parameters(CommitParams { rig }))
                    .await
            }
            CliTool::Rig {
                action,
                rig,
                source_dir,
            } => {
                server
                    .rig(mcp::Parameters(RigParams {
                        action: action.as_str().to_string(),
                        rig,
                        source_dir,
                    }))
                    .await
            }
        };
        match result {
            Ok(r) => match r.structured_content {
                Some(value) => {
                    let failed = value.get("error").is_some();
                    println!("{value}");
                    if failed { 1 } else { 0 }
                }
                None => 0,
            },
            Err(e) => {
                eprintln!("{e}");
                1
            }
        }
    }
    .await;
    process::ExitCode::from(exit_code as u8)
}

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
