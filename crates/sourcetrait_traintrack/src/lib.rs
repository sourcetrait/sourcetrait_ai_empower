pub(crate) mod cli {
    pub(crate) mod cli;
}
pub(crate) mod error;
pub(crate) mod run;

pub use crate::{
    cli::cli::{
        Cli, MaterialKind, NewCmd, OpenCmd, CloseCmd, FetchCmd,
    },
    error::{TraintrackError, TraintrackResult},
    run::{
        run_main, run,
    },
};

pub(crate) use std::{
    path::PathBuf,
    process::ExitCode,
};

pub(crate) use sourcetrait_common::{
    clapx,
    datum,
};

pub(crate) use clap::Parser;