pub(crate) mod context;
pub(crate) mod error;
pub(crate) mod input;
pub(crate) mod layout;
pub(crate) mod run;
pub(crate) mod store;

pub(crate) use std::{
    env,
    fs,
    io::{
        self,
        BufRead,
        Read,
    },
    os::unix::fs::symlink,
    path::{
        Path,
        PathBuf,
    },
};

pub(crate) use snafu::ResultExt;

pub(crate) use sourcetrait_ai_lib_empower as lib;

pub(crate) use crate::{
    context::{
        ContextModel,
        ContextSchemaChanged,
    },
    error::{
        ClaudelineError,
        ClaudelineResult,
        FsSnafu,
        SerializeYamlSnafu,
    },
    input::Input,
    layout::{
        LayoutKind,
        RateWindow,
        RenderInput,
        render,
    },
    store::persist_session,
};

pub use crate::run::run;
