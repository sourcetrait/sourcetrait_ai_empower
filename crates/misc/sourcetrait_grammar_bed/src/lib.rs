pub(crate) mod commands;
pub(crate) mod config;
pub(crate) mod engine;
pub(crate) mod error;

pub(crate) use crate::{
    commands::base_engine_state,
    error::{
        CompileSnafu, EvalSnafu, MissingMainSnafu, ParseSnafu, PanickedSnafu, ScriptReadSnafu,
        SetupSnafu, TimeoutSnafu,
    },
};

pub(crate) use snafu::ResultExt;

pub(crate) use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
    sync::{
        Arc,
        atomic::{
            AtomicBool,
            Ordering,
        },
        mpsc,
    },
    thread,
    time::Duration,
};

pub(crate) mod nu {
    pub(crate) use nu_engine::{
        eval_block,
        eval_block_with_early_return,
    };
    pub(crate) use nu_parser::parse;
    pub(crate) use nu_protocol::{
        PipelineData,
        ShellError,
        Signals,
        Span,
        Type,
        VarId,
        ast::Block,
        debugger::WithoutDebug,
        engine::{
            EngineState,
            Stack,
            StateWorkingSet,
        },
    };
}

pub use crate::{
    config::{
        BedConfig,
        BedConfigBuilder,
    },
    engine::Bed,
    error::{
        BedError,
        BedResult,
    },
};
pub use nu_protocol::{
    Record,
    Span,
    Value,
    record,
};
