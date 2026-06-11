pub(crate) mod base62;
pub(crate) mod error;
pub(crate) mod markdown;
pub mod consts;
pub mod nonce;
pub mod rerun;

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

pub(crate) mod xxh3 {
    pub(crate) use xxhash_rust::xxh3::Xxh3;
}

pub use crate::{
    base62::is_base62,
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
