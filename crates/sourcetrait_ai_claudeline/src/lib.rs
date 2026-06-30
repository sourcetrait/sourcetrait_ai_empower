pub(crate) mod claude_session_nom;
pub(crate) mod context;
pub(crate) mod error;
pub(crate) mod input;
pub(crate) mod layout;
pub(crate) mod pid;
pub(crate) mod run;
pub(crate) mod store;

pub(crate) use std::{
    env,
    fmt::Display,
    fs,
    hash::{
        Hash,
        Hasher,
    },
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

pub(crate) mod xxh3 {
    pub(crate) use xxhash_rust::xxh3::Xxh3;
}

pub(crate) use crate::{
    claude_session_nom::ClaudeSessionNom,
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
    input::{
        Input,
        ai_identity,
    },
    layout::{
        LayoutKind,
        RateWindow,
        RenderInput,
        render,
    },
    store::persist_session,
};

pub use crate::run::run;

#[cfg(test)]
mod tests {
    mod pid;
}
