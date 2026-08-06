use crate::*;

/// The file the embedded API appends to, inside this call's nonce log dir.
pub(crate) const DEBUG_FILE: &str = "debug.nuonl";

/// Per-eval state every `grimm *` decl carries: where this call's artifacts go.
#[derive(Clone)]
pub(crate) struct NuapiCall {
    log_dir: PathBuf,
    origin: String,
}

impl NuapiCall {
    pub(crate) fn new(log_dir: PathBuf) -> Self {
        let nonce = log_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            log_dir,
            origin: format!("thread/{nonce}"),
        }
    }

    /// The host-stamped `from` for anything this eval emits.
    pub(crate) fn origin(&self) -> String {
        self.origin.clone()
    }

    /// Append `data` to `<log_dir>/debug.nuonl` as ONE line of NUON.
    pub(crate) fn append_debug(
        &self,
        data: &nu::Value,
        span: nu::Span,
    ) -> Result<(), nu::ShellError> {
        let rendered = nu::to_nuon(&nu::EngineState::new(), data, nu::ToNuonConfig::default())
            .map_err(|e| {
                nu::GenericError::new("cannot render the value as NUON", e.to_string(), span)
            })?;
        let line = rendered.replace('\n', "\\n").replace('\r', "\\r");
        append_line(&self.log_dir.join(DEBUG_FILE), &line).map_err(|e| {
            nu::GenericError::new(format!("cannot append to {DEBUG_FILE}"), e.to_string(), span)
        })?;
        Ok(())
    }
}

/// Enforce the declared `oneof<record, table>` at RUNTIME.
pub(crate) fn require_record_or_table(
    data: &nu::Value,
    span: nu::Span,
) -> Result<(), nu::ShellError> {
    match data {
        nu::Value::Record { .. } => Ok(()),
        nu::Value::List { vals, .. }
            if vals.iter().all(|v| matches!(v, nu::Value::Record { .. })) =>
        {
            Ok(())
        }
        other => Err(nu::GenericError::new(
            "expected a record or a table",
            format!("got {}", other.get_type()),
            span,
        )
        .into()),
    }
}

/// The `data` positional shared by the `grimm *` decls: record or table.
pub(crate) fn data_shape() -> nu::SyntaxShape {
    nu::SyntaxShape::OneOf(vec![
        nu::SyntaxShape::Record(nu::CollectionColumns::from(vec![])),
        nu::SyntaxShape::Table(nu::CollectionColumns::from(vec![])),
    ])
}

/// The nullable `data` positional shared by the `grimm *` decls: record or table.
#[allow(unused)]
pub(crate) fn optional_data_shape() -> nu::SyntaxShape {
    nu::SyntaxShape::OneOf(vec![
        nu::SyntaxShape::Record(nu::CollectionColumns::from(vec![])),
        nu::SyntaxShape::Table(nu::CollectionColumns::from(vec![])),
        nu::SyntaxShape::Nothing,
    ])
}

/// Register the `grimm *` decls into the working set an eval will parse with.
pub(crate) fn register_nuapi(
    working_set: &mut nu::StateWorkingSet,
    log_dir: &std::path::Path,
) {
    let call = NuapiCall::new(log_dir.to_path_buf());
    working_set.add_decl(Box::new(GrimmDbg::new(call.clone())));
    working_set.add_decl(Box::new(GrimmChannelSend::new(call)));
    working_set.add_decl(Box::new(GrimmGetConfigAll));
    working_set.add_decl(Box::new(GrimmGetConfig));
    working_set.add_decl(Box::new(GrimmPinConfig));
    working_set.add_decl(Box::new(GrimmRemoteChannelSend));
    working_set.add_decl(Box::new(GrimmRemoteChannelSendWith));
}
