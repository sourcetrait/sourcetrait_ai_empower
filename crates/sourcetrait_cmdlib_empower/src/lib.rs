pub(crate) mod error;
pub mod markdown;

pub(crate) use std::path::{Path, PathBuf};
pub(crate) use snafu::{ResultExt, Snafu};

pub use crate::{
    error::{
        CmdEmpowerError,
        CmdEmpowerResult,
    },
    markdown::{
        find,
        FindError,
        FindResult,
    },
};
