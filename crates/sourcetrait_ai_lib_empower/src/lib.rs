pub(crate) mod base62;
pub(crate) mod error;
pub(crate) mod markdown;
pub(crate) mod claude_session_nom;
pub mod consts;
pub(crate) mod nonce;
pub(crate) mod rerun;

pub(crate) use std::{
    fmt::Display,
    hash::{
        Hash,
        Hasher,
    },
    path::Path,
    sync::atomic::{
        AtomicUsize,
        Ordering,
    },
    time::{
        SystemTime,
        UNIX_EPOCH,
    },
};

pub(crate) use snafu::ResultExt;

pub(crate) use crate::error::{
    InvalidPatternSnafu,
    ReadFileSnafu,
};

pub(crate) mod xxh3 {
    pub(crate) use xxhash_rust::xxh3::Xxh3;
}

pub use crate::{
    base62::is_base62,
    claude_session_nom::ClaudeSessionNom,
    error::{
        LibEmpowerError,
        LibEmpowerResult,
        MarkdownError,
        MarkdownResult,
    },
    nonce::{
        Nonce,
        NonceGen,
    },
    rerun::RerunHash,
};

pub mod md {
    pub use crate::{
        error::{
            MarkdownError,
            MarkdownResult,
        },
        markdown::find,
    };
}
