use crate::*;

/// The file the embedded API appends to, inside the eval's own nonce log dir.
pub(crate) const DEBUG_FILE: &str = "debug.nuonl";

/// Per-eval state every `grimm *` decl carries: where THIS call's artifacts go.
///
/// The log dir is a plain Rust value `eval_in_process` already holds. It is not
/// in engine state, not in `$env`, and nothing is seeded into the body's
/// environment to carry it - each decl is registered into the working set that
/// eval is about to parse with, so the dir rides on the decl and the existing
/// render/merge_delta carries it at no extra cost.
///
/// A stateless eval builds its own working set, so its dir is isolated by
/// construction even under full concurrency. The interact lane is serial and its
/// engine persists, so re-registering shadows the previous decl each call - one
/// decl of storage per call, the same bounded and already-accepted growth as its
/// `vars` table, cleared by a lane respawn.
#[derive(Clone)]
pub(crate) struct NuapiCall {
    log_dir: PathBuf,
    origin: String,
}

impl NuapiCall {
    pub(crate) fn new(log_dir: PathBuf) -> Self {
        // The log dir is NAMED for the eval's nonce, so the packet origin falls out of
        // the path this already carries - nothing extra threads through the eval
        // signatures to get it. The nonce is random enough that run / call / interact
        // need no distinguishing.
        let nonce = log_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            log_dir,
            origin: format!("thread/{nonce}"),
        }
    }

    /// The host-stamped `from` for anything this eval emits. A body cannot forge it,
    /// because it never supplies it.
    pub(crate) fn origin(&self) -> String {
        self.origin.clone()
    }

    /// Append `data` to `<log_dir>/debug.nuonl` as ONE line of NUON.
    ///
    /// `to nuon` renders compactly - no pretty-printing newlines between fields -
    /// but it does NOT escape a newline INSIDE a string value; it emits the byte
    /// raw. Verified, not assumed: `{msg: "a\nb"}` comes back out of `to nuon`
    /// spanning two lines, which would break the one-record-per-line invariant a
    /// nuonl reader depends on. So raw newline / carriage-return bytes are
    /// re-escaped here.
    ///
    /// That substitution is safe precisely because `data` is a record or table and
    /// the render is compact: the only raw newlines the output can carry are inside
    /// double-quoted strings, where `\n` / `\r` ARE the escapes nushell reads back -
    /// so the line still parses to the original value. Backslashes `to nuon` already
    /// escaped are untouched, since only the newline bytes themselves are replaced.
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
///
/// The signature's `SyntaxShape` rejects a bad LITERAL at parse time, but a
/// dynamic argument (`grimm dbg $x`) reaches `run` unchecked - records are open and
/// a list is not shape-checked per element - so the guard lives here too. An empty
/// list passes: `[]` satisfies any table, matching the schema system's rule.
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

/// The `data` positional shared by every `grimm *` decl that takes one:
/// `oneof<record, table>`, both column-open (an empty `CollectionColumns`
/// declares no required columns, so any record or table binds).
pub(crate) fn data_shape() -> nu::SyntaxShape {
    nu::SyntaxShape::OneOf(vec![
        nu::SyntaxShape::Record(nu::CollectionColumns::from(vec![])),
        nu::SyntaxShape::Table(nu::CollectionColumns::from(vec![])),
    ])
}

/// Register the `grimm *` embedded API into the working set an eval is about to
/// parse with.
///
/// MUST run before `nu::parse` - the parser resolves command names at parse time,
/// so a decl added afterwards is invisible to the body that needed it.
pub(crate) fn register_nuapi(
    working_set: &mut nu::StateWorkingSet,
    log_dir: &std::path::Path,
) {
    let call = NuapiCall::new(log_dir.to_path_buf());
    working_set.add_decl(Box::new(GrimmDbg::new(call.clone())));
    working_set.add_decl(Box::new(GrimmChannelSend::new(call)));
    // The config trio carries no per-call state - it reads process-global config
    // and the pin layer - so these take no `NuapiCall`. They register here anyway
    // rather than on the base, because this one site is what keeps the whole
    // `grimm` family unreachable outside an eval.
    working_set.add_decl(Box::new(GrimmGetConfigAll));
    working_set.add_decl(Box::new(GrimmGetConfig));
    working_set.add_decl(Box::new(GrimmPinConfig));
}
