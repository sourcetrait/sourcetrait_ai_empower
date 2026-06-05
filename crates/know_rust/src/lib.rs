pub(crate) mod cli;
pub(crate) mod error;
pub(crate) mod run;
pub(crate) mod scan {
    pub(crate) mod helpers;
    pub(crate) mod items {
        pub(crate) mod filters;
        pub(crate) mod helpers;
        pub(crate) mod macros;
        pub(crate) mod run;
        pub(crate) mod types;
        pub(crate) mod walker;
        pub(crate) mod workspace;
    }
    pub(crate) mod usages {
        pub(crate) mod facts;
        pub(crate) mod run;
        pub(crate) mod scan;
        pub(crate) mod walk;
    }
}

pub(crate) use crate::{
    cli::*,
    error::*,
    scan::helpers::*,
    scan::items::filters::*,
    scan::items::helpers::*,
    scan::items::macros::*,
    scan::items::walker::*,
    scan::items::workspace::*,
    scan::usages::scan::*,
    scan::usages::walk::*,
};

pub(crate) use std::{
    collections::{
        BTreeMap,
        HashMap,
    },
    fs,
    io,
    path::{
        Path,
        PathBuf,
    },
};

pub(crate) use clap::Parser;

pub(crate) use syn::{
    spanned::Spanned,
    visit::Visit,
};

pub use crate::run::run;
pub use crate::scan::items::run::*;
pub use crate::scan::items::types::*;
pub use crate::scan::usages::facts::*;
pub use crate::scan::usages::run::*;
