use crate::*;

/// RFC 6455 policy violation - the peer was emitted to before verifying.
const CLOSE_UNVERIFIED_EMIT: u16 = 1008;
const UNVERIFIED_EMIT_REASON: &str = "emit before verification";

/// `grimm channel_send <model> <event> [attached]` - the state-update lane.
#[derive(Clone)]
pub(crate) struct GrimmChannelSend {
    call: NuapiCall,
}

impl GrimmChannelSend {
    pub(crate) fn new(call: NuapiCall) -> Self {
        Self { call }
    }
}

impl nu::Command for GrimmChannelSend {
    fn name(&self) -> &str {
        "grimm channel_send"
    }

    fn signature(&self) -> nu::Signature {
        nu::Signature::build("grimm channel_send")
            .required("model", nu::SyntaxShape::String, "the shape the event carries")
            .required("event", data_shape(), "the state summary to notify")
            .optional("attached", data_shape(), "bulk to leave in the inbox")
            .input_output_types(vec![(nu::Type::Nothing, nu::Type::String)])
            .category(nu::Category::Custom("grimm".to_string()))
    }

    fn description(&self) -> &str {
        "Notify the agent that state changed; returns the message id."
    }

    fn run(
        &self,
        engine_state: &nu::EngineState,
        stack: &mut nu::Stack,
        call: &nu::Call<'_>,
        _input: nu::PipelineData,
    ) -> Result<nu::PipelineData, nu::ShellError> {
        let model: String = call.req(engine_state, stack, 0)?;
        if model.starts_with(MCP_RESERVED_PREFIX) {
            return Err(shell_error(
                &format!(
                    "`{MCP_RESERVED_PREFIX}` is reserved for host-originated models; \
                     choose a model path outside it",
                ),
                call.head,
            ));
        }
        let event: nu::Value = call.req(engine_state, stack, 1)?;
        require_record_or_table(&event, call.head)?;
        let attached: Option<nu::Value> = call.opt(engine_state, stack, 2)?;
        if let Some(value) = &attached {
            require_record_or_table(value, call.head)?;
        }

        let channel = channel_handle();
        let from = self.call.origin();

        match channel.status().phase {
            ChannelPhase::Closed => {
                return Err(shell_error(ChannelSendError::NotOpen.message(), call.head));
            }
            ChannelPhase::Open => {
                channel.close(CLOSE_UNVERIFIED_EMIT, UNVERIFIED_EMIT_REASON);
                return Err(shell_error(
                    ChannelSendError::NotVerified.message(),
                    call.head,
                ));
            }
            ChannelPhase::Verified => {}
        }

        match channel.record_send(&from) {
            SpamVerdict::Clear => {}
            SpamVerdict::Warn { hits, window_secs } => {
                let rate = channel.thresholds().warn_rate;
                channel.fire_emergency(Emergency::ChannelSpamWarning(
                    ChannelSpamWarningEmergency {
                        origin: from.clone(),
                        hits,
                        window_secs,
                        rate,
                    },
                ));
            }
            SpamVerdict::Stop {
                hits,
                window_secs,
                notify,
            } => {
                let rate = channel.thresholds().error_rate;
                let action = stop_offender(engine_state);
                if notify {
                    channel.fire_emergency(Emergency::ChannelSpamError(
                        ChannelSpamErrorEmergency {
                            origin: from.clone(),
                            hits,
                            window_secs,
                            rate,
                            action: action.clone(),
                        },
                    ));
                }
                return Err(shell_error(
                    &format!(
                        "channel send rate exceeded: {hits} in {window_secs}s from {from} \
                         (limit {rate}); the channel carries STATE SUMMARIES, not data - \
                         put bulk in `attached`, or write a nuon file",
                    ),
                    call.head,
                ));
            }
        }

        let event_nuon = render_nuon(&event).map_err(|e| shell_error(&e, call.head))?;
        let attached_nuon = match &attached {
            Some(value) => Some(render_nuon(value).map_err(|e| shell_error(&e, call.head))?),
            None => None,
        };
        let id = mint_msg_id(
            channel.nonce_gen(),
            &from,
            &model,
            &event_nuon,
            attached_nuon.as_deref(),
        );

        let attached_name = match &attached_nuon {
            Some(nuon) => Some(
                write_attachment(&channel, &id.to_string(), nuon)
                    .map_err(|e| shell_error(&e, call.head))?,
            ),
            None => None,
        };

        let line = render_packet(id, &from, &model, &event, attached_name.as_deref())
            .map_err(|e| shell_error(&e, call.head))?;
        channel
            .emit(line)
            .map_err(|e| shell_error(e.message(), call.head))?;
        Ok(nu::PipelineData::Value(
            nu::Value::string(id.to_string(), call.head),
            None,
        ))
    }
}

/// The reason goes in the title; only the title reaches the envelope message.
fn shell_error(
    message: &str,
    span: nu::Span,
) -> nu::ShellError {
    nu::GenericError::new(
        format!("grimm channel_send: {message}"),
        message.to_string(),
        span,
    )
    .into()
}

/// The root job: a foreground eval, owning no entry in the jobs table.
const ROOT_JOB_ID: usize = 0;

/// Stop the origin that just crossed the hard threshold.
fn stop_offender(engine_state: &nu::EngineState) -> String {
    engine_state.signals().trigger();
    let job_id = engine_state.current_job.id;
    if job_id.get() == ROOT_JOB_ID {
        return "signals triggered".to_string();
    }
    let mut jobs = engine_state.jobs.lock().unwrap_or_else(|e| e.into_inner());
    match jobs.kill_and_remove(job_id) {
        Ok(()) => format!("signals triggered; job {job_id} killed"),
        Err(e) => format!("signals triggered; job {job_id} removed, kill incomplete: {e}"),
    }
}

/// Write `attached` to `inbox/<id>.nuon` and return the NAME the wire carries.
fn write_attachment(
    channel: &ChannelHandle,
    id: &str,
    nuon: &str,
) -> Result<String, String> {
    let dir = channel
        .inbox()
        .ok_or_else(|| "the channel has no inbox; call channel_open() first".to_string())?;
    fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let name = format!("{id}.nuon");
    let path = dir.join(&name);
    fs::write(&path, nuon.as_bytes()).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(name)
}
