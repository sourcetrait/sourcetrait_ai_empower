use crate::*;

/// A CORE model drops the vendor prefix; everything else carries its own.
const MODEL_SPAM_WARNING: &str = "channel/spam/Warning";
const MODEL_SPAM_ERROR: &str = "channel/spam/Error";

/// RFC 6455 policy violation - the peer was emitted to before it proved itself.
const CLOSE_UNVERIFIED_EMIT: u16 = 1008;
const UNVERIFIED_EMIT_REASON: &str = "emit before verification";

/// `grimm channel_send <model> <event> [attached]` - the body's state-update lane.
///
/// THE CHANNEL IS A NOTIFICATION LANE, NOT A SERIALIZATION LANE. `event` says "state
/// changed, here is a tiny summary"; the agent fetches actual data itself. That is why
/// the signature has two slots: an author with something bulky puts it in `attached`,
/// which the host writes to the inbox and NAMES on the wire rather than carrying. The
/// contract is structural - the ergonomic path is the next argument, so nobody has to be
/// talked out of putting data on the wire.
///
/// Returns the message id, so an author can correlate what it sent with what the agent
/// later fetches.
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
        let event: nu::Value = call.req(engine_state, stack, 1)?;
        require_record_or_table(&event, call.head)?;
        let attached: Option<nu::Value> = call.opt(engine_state, stack, 2)?;
        if let Some(value) = &attached {
            require_record_or_table(value, call.head)?;
        }

        let channel = channel_handle();
        let from = self.call.origin();

        // Phase FIRST, before any rendering: a send that cannot happen should cost a
        // lock and an error, not a hash and a render.
        match channel.status().phase {
            ChannelPhase::Closed => {
                return Err(shell_error(ChannelSendError::NotOpen.message(), call.head));
            }
            ChannelPhase::Open => {
                // Telemetry must not reach a peer that has not proven it owns the stdio
                // session, and the channel is torn down for the attempt rather than
                // merely refused - an unproven peer that has been emitted to is not a
                // peer we keep.
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
                        from: from.clone(),
                        hits,
                        window_secs,
                        rate,
                    },
                ));
                notify_spam(&channel, MODEL_SPAM_WARNING, &from, hits, window_secs, rate, None);
            }
            SpamVerdict::Stop {
                hits,
                window_secs,
                notify,
            } => {
                let rate = channel.thresholds().error_rate;
                // SPAM IS THE ONE THING WE STOP, because it implies a BUG rather than
                // load. Both levers are needed: the throw kills an author who never
                // try/caught it, and the stop covers one who catches it and loops anyway.
                let action = stop_offender(engine_state);
                // ONCE per episode. The refusal below persists for every later send, but
                // announcing each one would turn the report about spam into spam.
                if notify {
                    channel.fire_emergency(Emergency::ChannelSpamError(
                        ChannelSpamErrorEmergency {
                            from: from.clone(),
                            hits,
                            window_secs,
                            rate,
                            action: action.clone(),
                        },
                    ));
                    notify_spam(
                        &channel,
                        MODEL_SPAM_ERROR,
                        &from,
                        hits,
                        window_secs,
                        rate,
                        Some(&action),
                    );
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

        // The id names the file, so it is minted before the write - which is also why the
        // hash covers the attached CONTENT rather than its path.
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

/// The REASON goes in the title, not only the label: a `GenericError` renders its title
/// through Display, so a bare "grimm channel_send" there would reach the agent with the
/// cause stripped off - which is the opaque-error failure this crate already knows well.
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

/// The root job: a foreground eval. It owns no entry in the jobs table, so `Signals` is
/// the only lever that reaches it.
const ROOT_JOB_ID: usize = 0;

/// Stop the origin that just crossed the hard threshold.
///
/// TWO LEVERS, because the offender has two shapes. `Signals::trigger` reaches a spam
/// loop whatever it runs under - such a loop invokes a decl over and over, so it polls at
/// every one of those boundaries and bails. But a job that OUTLIVED the eval that spawned
/// it has no `in_flight` entry left - `InFlightCleanup` removed that the moment the
/// dispatch returned - and the jobs table is the only place it is still reachable. A
/// spawned closure reads its OWN JobId here, so `current_job` names the actual offender
/// rather than its parent.
fn stop_offender(engine_state: &nu::EngineState) -> String {
    engine_state.signals().trigger();
    let job_id = engine_state.current_job.id;
    if job_id.get() == ROOT_JOB_ID {
        return "signals triggered".to_string();
    }
    let mut jobs = engine_state.jobs.lock().unwrap_or_else(|e| e.into_inner());
    match jobs.kill_and_remove(job_id) {
        Ok(()) => format!("signals triggered; job {job_id} killed"),
        // The table entry is dropped either way; only killing its processes can fail, and
        // an external the job left behind is the tree-kill machinery's to reap.
        Err(e) => format!("signals triggered; job {job_id} removed, kill incomplete: {e}"),
    }
}

/// Tell the agent about the abuse ON the channel, as well as through the emergency lane.
///
/// Carries no payload example: the agent is already being spammed by that, and it can
/// investigate the cause itself. Uses the control path, since a warning about traffic
/// must not itself be subject to the rate it is reporting.
#[allow(clippy::too_many_arguments)]
fn notify_spam(
    channel: &ChannelHandle,
    model: &str,
    from: &str,
    hits: u32,
    window_secs: u64,
    rate: u32,
    action: Option<&str>,
) {
    let span = nu::Span::unknown();
    let mut event = nu::Record::new();
    event.insert("origin", nu::Value::string(from.to_string(), span));
    event.insert("hits", nu::Value::int(hits as i64, span));
    event.insert("window_secs", nu::Value::int(window_secs as i64, span));
    event.insert("rate", nu::Value::int(rate as i64, span));
    if let Some(action) = action {
        event.insert("action", nu::Value::string(action.to_string(), span));
    }
    let event = nu::Value::record(event, span);
    let Ok(event_nuon) = render_nuon(&event) else {
        return;
    };
    let id = mint_msg_id(channel.nonce_gen(), FROM_MCP, model, &event_nuon, None);
    if let Ok(line) = render_packet(id, FROM_MCP, model, &event, None) {
        let _ = channel.send_control(line);
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
