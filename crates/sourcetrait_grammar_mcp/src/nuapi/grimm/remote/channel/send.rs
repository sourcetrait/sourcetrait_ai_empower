use crate::*;

/// `grimm remote_channel_send <mcp_nom> <model> <event>` - a message, no files.
#[derive(Clone)]
pub(crate) struct GrimmRemoteChannelSend;

/// `grimm remote_channel_send_with <mcp_nom> <model> <event> <attached>` - a
/// message plus files; `attached` is the files table, src -> receiver dest.
#[derive(Clone)]
pub(crate) struct GrimmRemoteChannelSendWith;

impl GrimmRemoteChannelSend {
    pub(crate) const DEF_MCP_NOM: nuin::ParameterDef = nuin::ParameterDef {
        name: "mcp_nom",
        description: "Remote MCP",
    };
    pub(crate) const DEF_MODEL: nuin::ParameterDef = nuin::ParameterDef {
        name: "model",
        description: "Namepath of the event data type",
    };
    pub(crate) const DEF_EVENT: nuin::ParameterDef = nuin::ParameterDef {
        name: "event",
        description: "Notification data",
    };
}

impl nu::Command for GrimmRemoteChannelSend {
    fn name(&self) -> &str { Self::DEF.name }

    fn signature(&self) -> nu::Signature {
        nu::Signature::build(Self::DEF.name)
            .vocab(&Self::DEF)
            .vocab_required(&Self::DEF_MCP_NOM, nu::SyntaxShape::String)
            .vocab_required(&Self::DEF_MODEL, nu::SyntaxShape::String)
            .vocab_required(&Self::DEF_EVENT, data_shape())
            .input_output_types(vec![(nu::Type::Nothing, nu::Type::String)])
    }

    fn description(&self) -> &str { Self::DEF.description }

    fn run(
        &self,
        engine_state: &nu::EngineState,
        stack: &mut nu::Stack,
        call: &nu::Call<'_>,
        _input: nu::PipelineData,
    ) -> Result<nu::PipelineData, nu::ShellError> {
        let mcp_nom: String = call.req(engine_state, stack, 0)?;
        let model: String = call.req(engine_state, stack, 1)?;
        reject_reserved_model(&model).map_err(|e| shell_error(&e, call.head))?;
        let event: nu::Value = call.req(engine_state, stack, 2)?;
        require_record_or_table(&event, call.head)?;
        let event_nuon = render_nuon(&event).map_err(|e| shell_error(&e, call.head))?;
        let id = mint_remote_id(&model, &event_nuon);
        let (wire_event_nuon, payloads) =
            spill_remote_event(event_nuon, Vec::new()).map_err(|e| shell_error(&e, call.head))?;
        find_link_send(&mcp_nom, id.clone(), model, wire_event_nuon, payloads)
            .map_err(|e| shell_error(&e, call.head))?;
        Ok(nu::PipelineData::Value(nu::Value::string(id, call.head), None))
    }
}

impl nu::Command for GrimmRemoteChannelSendWith {
    fn name(&self) -> &str {
        "grimm remote_channel_send_with"
    }

    fn signature(&self) -> nu::Signature {
        nu::Signature::build("grimm remote_channel_send_with")
            .required("mcp_nom", nu::SyntaxShape::String, "the linked peer host's McpNom")
            .required("model", nu::SyntaxShape::String, "the shape the event carries")
            .required("event", data_shape(), "the state summary to deliver")
            .required(
                "attached",
                nu::SyntaxShape::Table(nu::CollectionColumns::from(vec![])),
                "the files table<src: path, dest: path>: local src -> receiver dest name",
            )
            .input_output_types(vec![(nu::Type::Nothing, nu::Type::String)])
            .category(nu::Category::Custom("grimm".to_string()))
    }

    fn description(&self) -> &str {
        "Deliver a message plus files to a linked remote host's agent; returns the message id."
    }

    fn run(
        &self,
        engine_state: &nu::EngineState,
        stack: &mut nu::Stack,
        call: &nu::Call<'_>,
        _input: nu::PipelineData,
    ) -> Result<nu::PipelineData, nu::ShellError> {
        let mcp_nom: String = call.req(engine_state, stack, 0)?;
        let model: String = call.req(engine_state, stack, 1)?;
        reject_reserved_model(&model).map_err(|e| shell_error(&e, call.head))?;
        let event: nu::Value = call.req(engine_state, stack, 2)?;
        require_record_or_table(&event, call.head)?;
        let attached: nu::Value = call.req(engine_state, stack, 3)?;
        let payloads = read_attachments(&attached).map_err(|e| shell_error(&e, call.head))?;
        let event_nuon = render_nuon(&event).map_err(|e| shell_error(&e, call.head))?;
        let id = mint_remote_id(&model, &event_nuon);
        let (wire_event_nuon, payloads) =
            spill_remote_event(event_nuon, payloads).map_err(|e| shell_error(&e, call.head))?;
        find_link_send(&mcp_nom, id.clone(), model, wire_event_nuon, payloads)
            .map_err(|e| shell_error(&e, call.head))?;
        Ok(nu::PipelineData::Value(nu::Value::string(id, call.head), None))
    }
}

/// Read each `{src, dest}` row into `(dest, bytes)`, validating the dest and
/// reading the local src synchronously (a read failure fails the send outright,
/// before the id is returned - a local problem the agent learns immediately).
fn read_attachments(value: &nu::Value) -> Result<Vec<(String, Vec<u8>)>, String> {
    let rows = match value {
        nu::Value::List { vals, .. } => vals,
        _ => return Err("attached must be a table<src: path, dest: path>".to_string()),
    };
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let rec = match row {
            nu::Value::Record { val, .. } => val,
            _ => return Err("each attached row must be a record with `src` and `dest`".to_string()),
        };
        let src = rec
            .get("src")
            .and_then(|v| v.as_str().ok())
            .ok_or_else(|| "each attached row needs a `src` path".to_string())?;
        let dest = rec
            .get("dest")
            .and_then(|v| v.as_str().ok())
            .ok_or_else(|| "each attached row needs a `dest` name".to_string())?;
        if !safe_dest(dest) {
            return Err(format!("dest `{dest}` must be a relative path with no `..`"));
        }
        let bytes = fs::read(src).map_err(|e| format!("read src `{src}`: {e}"))?;
        out.push((dest.to_string(), bytes));
    }
    Ok(out)
}

/// Mint the send's message id off the shared channel NonceGen (a unique base62).
fn mint_remote_id(
    model: &str,
    event_nuon: &str,
) -> String {
    mint_msg_id(channel_handle().nonce_gen(), "remote/send", model, event_nuon, None).to_string()
}

/// CapNoCap (sender side): if the rendered event would overflow the receiver's
/// notification cap, move it to a transferred file (the reserved EVENT_SPILL_DEST,
/// alongside any caller files) and return a compact pointer event in its place.
/// The receiver rewrites the pointer's path under this delivery's inbox directory
/// before relaying, so the agent resolves it as <inbox>/<spilled_event_path>. Via
/// the file connection, so a >1 MiB event (which no Deliver frame could carry
/// inline) still reaches the agent. Returns (wire event NUON, payloads).
fn spill_remote_event(
    event_nuon: String,
    mut payloads: Vec<(String, Vec<u8>)>,
) -> Result<(String, Vec<(String, Vec<u8>)>), String> {
    if !event_overflows(&event_nuon) {
        return Ok((event_nuon, payloads));
    }
    let pointer_nuon = render_nuon(&event_spill_pointer(EVENT_SPILL_DEST, event_nuon.len()))?;
    payloads.push((EVENT_SPILL_DEST.to_string(), event_nuon.into_bytes()));
    Ok((pointer_nuon, payloads))
}

/// The `mcp/` reservation is host-origin only; an agent send picks another path.
fn reject_reserved_model(model: &str) -> Result<(), String> {
    if model.starts_with(MCP_RESERVED_PREFIX) {
        return Err(format!(
            "`{MCP_RESERVED_PREFIX}` is reserved for host-originated models; \
             choose a model path outside it",
        ));
    }
    Ok(())
}

/// The reason goes in the title; only the title reaches the envelope message.
fn shell_error(
    message: &str,
    span: nu::Span,
) -> nu::ShellError {
    nu::GenericError::new(
        format!("grimm remote_channel_send: {message}"),
        message.to_string(),
        span,
    )
    .into()
}
