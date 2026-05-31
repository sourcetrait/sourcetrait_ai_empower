pub(crate) mod error;
pub mod consts;
pub mod nonce;

pub(crate) use std::{
    fmt::Display,
    hash::{
        Hash,
        Hasher,
    },
    sync::atomic::{
        AtomicUsize,
        Ordering,
    },
    time::{
        SystemTime,
        UNIX_EPOCH,
    },
};

pub(crate) mod xxh3 {
    pub(crate) use xxhash_rust::xxh3::Xxh3;
}

pub use crate::{
    error::{
        LibEmpowerError,
        LibEmpowerResult,
    },
    nonce::{
        Nonce,
        NonceGen,
    },
};