pub(crate) mod characterize {
    pub(crate) mod cargo_toml;
    pub(crate) mod components;
    pub(crate) mod items_index;
    pub(crate) mod mode;
    pub(crate) mod pattern_histogram;
    pub(crate) mod pattern_metrics;
    pub(crate) mod run;
    pub(crate) mod scan_crate;
    pub(crate) mod shape;
    pub(crate) mod sloc;
    pub(crate) mod types;
    pub(crate) mod use_classification;
}
pub(crate) mod cli;
pub(crate) mod emit {
    pub(crate) mod cluster;
    pub(crate) mod container_routing;
    pub(crate) mod contexts;
    pub(crate) mod instance;
    pub(crate) mod orientation;
    pub(crate) mod picker;
    pub(crate) mod reference;
    pub(crate) mod run;
    pub(crate) mod seams;
    pub(crate) mod spans;
}
pub(crate) mod config {
    pub(crate) mod calibration;
    pub(crate) mod loader;
    pub(crate) mod templates;
}
pub(crate) mod error;
pub(crate) mod measure_overlap {
    pub(crate) mod parse;
    pub(crate) mod run;
    pub(crate) mod score;
    pub(crate) mod types;
}
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
    characterize::cargo_toml::*,
    characterize::components::*,
    characterize::items_index::*,
    characterize::mode::*,
    characterize::pattern_histogram::*,
    characterize::pattern_metrics::*,
    characterize::scan_crate::*,
    characterize::shape::*,
    characterize::sloc::*,
    characterize::types::*,
    characterize::use_classification::*,
    cli::*,
    emit::cluster::*,
    emit::container_routing::*,
    emit::contexts::*,
    emit::instance::*,
    emit::orientation::*,
    emit::picker::*,
    emit::reference::*,
    emit::seams::*,
    emit::spans::*,
    error::*,
    measure_overlap::parse::*,
    measure_overlap::score::*,
    measure_overlap::types::*,
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
        HashSet,
    },
    fs,
    io,
    path::{
        Path,
        PathBuf,
    },
    process,
};

pub(crate) use clap::Parser;

pub(crate) use syn::{
    spanned::Spanned,
    visit::Visit,
};

pub use crate::run::run;
pub use crate::characterize::run::*;
pub use crate::emit::run::*;
pub use crate::measure_overlap::run::*;
pub use crate::config::calibration::*;
pub use crate::config::loader::*;
pub use crate::config::templates::*;
pub use crate::scan::items::run::*;
pub use crate::scan::items::types::*;
pub use crate::scan::usages::facts::*;
pub use crate::scan::usages::run::*;
