//! The body-facing config surface: read the settings in force, and pin one to the
//! lifetime of a process.
//!
//! The record these return mirrors the TOML - the same two tables, the same key
//! names - so one shape describes the file, the `.nutype` model beside it, and
//! what a body sees. The argument-owned values (`--id`, `--namespace`,
//! `--workdir`) are deliberately absent: they are not config-file settings, and a
//! body already reads them ambiently as the `EQUIP_*` trio.
//!
//! Values are live rather than as-launched. The channel's spam thresholds come
//! from the channel handle, which `config_channel` mutates, and the supervisor
//! lines come through the pin layer, so what a body reads is what is actually in
//! force rather than the startup seed.

use crate::*;

/// The dotted key separator, and the reason `get_config_all | get <key>` and
/// `get_config <key>` agree: the key is a path into the record the other decl
/// returns, not a parallel naming scheme.
const KEY_SEP: char = '.';

/// The whole config as a nu record, mirroring the TOML's two tables.
fn config_record() -> nu::Value {
    let span = nu::Span::unknown();
    let cfg = config();
    let spam = channel_handle().thresholds();
    let supervisor = effective_supervisor();

    let mut channel = nu::Record::new();
    channel.insert(
        "port",
        match cfg.channel.port {
            Some(port) => nu::Value::int(port as i64, span),
            None => nu::Value::nothing(span),
        },
    );
    channel.insert(
        "cert_dir",
        nu::Value::string(cfg.channel.cert_dir.to_string_lossy().into_owned(), span),
    );
    channel.insert(
        "spam_warn_window_secs",
        nu::Value::int(spam.warn_window.as_secs() as i64, span),
    );
    channel.insert("spam_warn_rate", nu::Value::int(spam.warn_rate as i64, span));
    channel.insert(
        "spam_error_window_secs",
        nu::Value::int(spam.error_window.as_secs() as i64, span),
    );
    channel.insert(
        "spam_error_rate",
        nu::Value::int(spam.error_rate as i64, span),
    );

    let mut sup = nu::Record::new();
    sup.insert(
        "cpu_warn_fraction",
        nu::Value::float(supervisor.cpu_warn_fraction, span),
    );
    sup.insert(
        "ram_warn_fraction",
        nu::Value::float(supervisor.ram_warn_fraction, span),
    );
    sup.insert(
        "vram_warn_headroom_mib",
        nu::Value::int(supervisor.vram_warn_headroom_mib as i64, span),
    );
    sup.insert(
        "disk_warn_fraction",
        nu::Value::float(supervisor.disk_warn_fraction, span),
    );

    let mut root = nu::Record::new();
    root.insert("channel", nu::Value::record(channel, span));
    root.insert("supervisor", nu::Value::record(sup, span));
    nu::Value::record(root, span)
}

/// Every dotted key the record holds, in order - the vocabulary an error message
/// needs, derived from the record rather than restated beside it.
fn config_keys() -> Vec<String> {
    let mut keys = Vec::new();
    let root = config_record();
    let Ok(tables) = root.as_record() else {
        return keys;
    };
    for (table, value) in tables {
        match value.as_record() {
            Ok(fields) => {
                for (field, _) in fields {
                    keys.push(format!("{table}{KEY_SEP}{field}"));
                }
            }
            Err(_) => keys.push(table.clone()),
        }
    }
    keys
}

/// Walk the record by a dotted key.
fn lookup(key: &str) -> Option<nu::Value> {
    let mut current = config_record();
    for segment in key.split(KEY_SEP) {
        let next = current.as_record().ok()?.get(segment)?.clone();
        current = next;
    }
    Some(current)
}

/// The vocabulary goes in the title, not only the label.
///
/// `GenericError` renders its title through Display, and that is all the eval
/// envelope's `message` carries - a label reaches a human reading a rendered
/// diagnostic and never reaches the agent. Listing the valid keys is the entire
/// value of this error, so putting them in the label would have shipped an error
/// that names a problem and withholds the answer. Caught by a test that asserted
/// the list was present and found it stripped.
fn unknown_key(
    key: &str,
    span: nu::Span,
) -> nu::ShellError {
    let keys = config_keys().join(", ");
    nu::GenericError::new(
        format!("grimm get_config: `{key}` is not a config key; valid keys: {keys}"),
        format!("valid keys: {keys}"),
        span,
    )
    .into()
}

/// The value type `get_config` can yield. Spelled as a union rather than `any`,
/// because the set is closed and naming it lets a caller's own annotation be
/// strict too.
fn value_type() -> nu::Type {
    nu::Type::one_of([
        nu::Type::Int,
        nu::Type::Float,
        nu::Type::String,
        nu::Type::Nothing,
    ])
}

/// `grimm get_config_all` - every setting in force, as one record.
///
/// The record mirrors the TOML, so `get_config_all | get supervisor.cpu_warn_fraction`
/// and `get_config "supervisor.cpu_warn_fraction"` are the same question asked two
/// ways.
#[derive(Clone)]
pub(crate) struct GrimmGetConfigAll;

impl nu::Command for GrimmGetConfigAll {
    fn name(&self) -> &str {
        "grimm get_config_all"
    }

    fn signature(&self) -> nu::Signature {
        nu::Signature::build("grimm get_config_all")
            // An open record: the shape is documented by the `.nutype` model
            // beside the defaults, not restated here where it would be a second
            // place to keep in step.
            .input_output_types(vec![(nu::Type::Nothing, nu::Type::record())])
            .category(nu::Category::Custom("grimm".to_string()))
    }

    fn description(&self) -> &str {
        "The host's configuration as it currently applies, mirroring the TOML."
    }

    fn run(
        &self,
        _engine_state: &nu::EngineState,
        _stack: &mut nu::Stack,
        _call: &nu::Call<'_>,
        _input: nu::PipelineData,
    ) -> Result<nu::PipelineData, nu::ShellError> {
        Ok(nu::PipelineData::Value(config_record(), None))
    }
}

/// `grimm get_config <cfg_key>` - one setting, by its dotted key.
#[derive(Clone)]
pub(crate) struct GrimmGetConfig;

impl nu::Command for GrimmGetConfig {
    fn name(&self) -> &str {
        "grimm get_config"
    }

    fn signature(&self) -> nu::Signature {
        nu::Signature::build("grimm get_config")
            .required(
                "cfg_key",
                nu::SyntaxShape::String,
                "the dotted key, e.g. supervisor.vram_warn_headroom_mib",
            )
            .input_output_types(vec![(nu::Type::Nothing, value_type())])
            .category(nu::Category::Custom("grimm".to_string()))
    }

    fn description(&self) -> &str {
        "One configuration setting by its dotted key; errors on an unknown key."
    }

    fn run(
        &self,
        engine_state: &nu::EngineState,
        stack: &mut nu::Stack,
        call: &nu::Call<'_>,
        _input: nu::PipelineData,
    ) -> Result<nu::PipelineData, nu::ShellError> {
        let key: String = call.req(engine_state, stack, 0)?;
        // An unknown key is an error rather than a null, so a typo cannot read as
        // "configured to nothing" - the two are indistinguishable at the call site
        // and only one of them is a bug the caller wants to hear about.
        let value = lookup(&key).ok_or_else(|| unknown_key(&key, call.head))?;
        Ok(nu::PipelineData::Value(value, None))
    }
}

/// `grimm pin_config <cfg_key> <process_id> <value>` - hold a setting for as long
/// as a process lives.
///
/// Only the `[supervisor]` warning lines are pinnable. `[channel]` is excluded so
/// `config_channel` stays the sole mutator there; everything else is not a runtime
/// value at all.
#[derive(Clone)]
pub(crate) struct GrimmPinConfig;

impl nu::Command for GrimmPinConfig {
    fn name(&self) -> &str {
        "grimm pin_config"
    }

    fn signature(&self) -> nu::Signature {
        nu::Signature::build("grimm pin_config")
            .required(
                "cfg_key",
                nu::SyntaxShape::String,
                "the dotted key to pin; the supervisor warning lines only",
            )
            .required(
                "process_id",
                nu::SyntaxShape::Int,
                "the pid whose lifetime the pin follows",
            )
            .required("value", nu::SyntaxShape::Number, "the value to hold")
            .input_output_types(vec![(nu::Type::Nothing, nu::Type::Nothing)])
            .category(nu::Category::Custom("grimm".to_string()))
    }

    fn description(&self) -> &str {
        "Hold a supervisor setting at a value for the lifetime of a process."
    }

    fn run(
        &self,
        engine_state: &nu::EngineState,
        stack: &mut nu::Stack,
        call: &nu::Call<'_>,
        _input: nu::PipelineData,
    ) -> Result<nu::PipelineData, nu::ShellError> {
        let key: String = call.req(engine_state, stack, 0)?;
        let pid: i64 = call.req(engine_state, stack, 1)?;
        let raw: nu::Value = call.req(engine_state, stack, 2)?;
        let value = match &raw {
            nu::Value::Int { val, .. } => *val as f64,
            nu::Value::Float { val, .. } => *val,
            other => {
                return Err(nu::GenericError::new(
                    "grimm pin_config: value must be a number",
                    format!("got {}", other.get_type()),
                    call.head,
                )
                .into());
            }
        };
        let pid = u32::try_from(pid).map_err(|_| {
            nu::GenericError::new(
                "grimm pin_config: process_id must be a positive process id",
                format!("got {pid}"),
                call.head,
            )
        })?;
        // The reason goes in the title, not only the label: GenericError renders
        // its title through Display, so a bare command name there reaches the
        // agent with the cause stripped off.
        pin(&key, pid, value).map_err(|reason| {
            nu::GenericError::new(
                format!("grimm pin_config: {reason}"),
                reason.clone(),
                call.head,
            )
        })?;
        Ok(nu::PipelineData::Empty)
    }
}
