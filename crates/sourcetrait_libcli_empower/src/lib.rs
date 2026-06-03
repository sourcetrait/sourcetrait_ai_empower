pub(crate) mod error;
pub(crate) mod markdown;

pub(crate) use std::path::{Path, PathBuf};
pub(crate) use snafu::{ResultExt, Snafu};

pub use crate::{
    error::{
        CmdEmpowerError,
        CmdEmpowerResult,
    },
};

pub mod md {
    pub use crate::{
        markdown::{
            find,
            FindError,
            FindResult,
        },
    };
}
