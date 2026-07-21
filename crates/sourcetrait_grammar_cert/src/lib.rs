mod config;
mod error;
mod generate;
mod install;
mod run;
mod verify;

#[cfg(test)]
mod tests {
    mod config;
    mod generate;
    mod verify;
}

pub use crate::error::{CertError, Result};
pub use crate::run::run;

pub(crate) use crate::{
    config::CertGenConfig,
    generate::{CertFiles, generate},
    install::install,
    verify::verify,
};

pub(crate) use std::{
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
    process,
};
