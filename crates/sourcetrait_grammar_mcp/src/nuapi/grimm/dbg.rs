use crate::*;

/// `grimm dbg <data>` - append a record or table to this eval's `debug.nuonl`.
#[derive(Clone)]
pub(crate) struct GrimmDbg {
    call: NuapiCall,
}

impl GrimmDbg {
    pub(crate) fn new(call: NuapiCall) -> Self {
        Self { call }
    }
}

impl nu::Command for GrimmDbg {
    fn name(&self) -> &str {
        "grimm dbg"
    }

    fn signature(&self) -> nu::Signature {
        nu::Signature::build("grimm dbg")
            .required("data", data_shape(), "the record or table to append")
            .input_output_types(vec![(nu::Type::Nothing, nu::Type::Nothing)])
            .category(nu::Category::Custom("grimm".to_string()))
    }

    fn description(&self) -> &str {
        "Append a record or table to this eval's debug.nuonl in its nonce log dir."
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
        self.call.append_debug(&data, call.head)?;
        Ok(nu::PipelineData::Empty)
    }
}
