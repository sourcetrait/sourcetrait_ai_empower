//! The body-facing config surface: read what is in force, and pin a setting.

use crate::*;

/// The dotted key separator.
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

/// Every dotted key the record holds, in order.
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

/// The unknown-key error, listing the valid keys in its title.
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

/// The value type `get_config` can yield, as a closed union.
fn value_type() -> nu::Type {
    nu::Type::one_of([
        nu::Type::Int,
        nu::Type::Float,
        nu::Type::String,
        nu::Type::Nothing,
    ])
}

/// `grimm get_config_all` - every setting in force, as one record.
#[derive(Clone)]
pub(crate) struct GrimmGetConfigAll;

impl nu::Command for GrimmGetConfigAll {
    fn name(&self) -> &str {
        "grimm get_config_all"
    }

    fn signature(&self) -> nu::Signature {
        nu::Signature::build("grimm get_config_all")
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
        let value = lookup(&key).ok_or_else(|| unknown_key(&key, call.head))?;
        Ok(nu::PipelineData::Value(value, None))
    }
}

/// `grimm pin_config` - hold a setting for as long as a process lives.
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
