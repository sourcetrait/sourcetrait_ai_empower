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
    store::{
        clear_latest,
        persist,
    },
};

pub use crate::run::run;
