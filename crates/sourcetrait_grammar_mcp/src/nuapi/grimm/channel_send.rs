use crate::*;

/// `grimm channel_send <data>` - the channel producer, STUBBED to `grimm dbg`.
///
/// This is the body-facing half of the channel campaign (followup #35): a
/// run/call/interact body emits a structured packet and the host fans it out to
/// the agent's Monitor. The transport does not exist yet, so for now it does
/// EXACTLY what `grimm dbg` does - appends the value to this eval's `debug.nuonl` -
/// giving the surface something real to be written against while the WebSocket
/// work lands. Behaviour changes under it; the call site does not.
///
/// The decl is registered per-eval into the working set (common.rs), so it exists
/// ONLY inside a run/call/interact body. That is the structural trap the channel
/// design wants: off-host nushell cannot resolve the name at all, let alone reach
/// the channel, so there is nothing to authenticate at this boundary.
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
            .required("data", data_shape(), "the record or table to send")
            .input_output_types(vec![(nu::Type::Nothing, nu::Type::Nothing)])
            .category(nu::Category::Custom("grimm".to_string()))
    }

    fn description(&self) -> &str {
        "Send a record or table on the host channel (currently stubbed to grimm dbg)."
    }

    fn run(
        &self,
        engine_state: &nu::EngineState,
        stack: &mut nu::Stack,
        call: &nu::Call<'_>,
        _input: nu::PipelineData,
    ) -> Result<nu::PipelineData, nu::ShellError> {
        let data: nu::Value = call.req(engine_state, stack, 0)?;
        require_record_or_table(&data, call.head)?;
        // Stub: identical to `grimm dbg` until the channel transport lands.
        self.call.append_debug(&data, call.head)?;
        Ok(nu::PipelineData::Empty)
    }
}
