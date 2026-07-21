mod config;
mod error;
mod generate;
mod install;
mod run;
mod store;
mod verify;

#[cfg(test)]
mod tests {
    mod config;
    mod generate;
    mod store;
    mod verify;
}

pub use crate::error::{CertError, Result};
pub use crate::run::run;

/// Test-only surface (the test-only-`pub` -> `guts` convention). `install` itself cannot
/// be driven from a test any more - it elevates through `sudo`, which needs an
/// interactive password - so the staging-cleanup decision is exercised directly.
pub mod guts {
    use crate::*;

    /// Run the staging inventory + removal against `dir`, returning `Ok(removed)` or the
    /// reason it was kept.
    pub fn consume_staging(
        dir: &std::path::Path,
        name: &str,
    ) -> std::result::Result<(), String> {
        let files = CertFiles::new(dir, name);
        match crate::install::consume_staging(dir, &files) {
            crate::install::Staging::Removed(_) => Ok(()),
            crate::install::Staging::Kept { reason, .. } => Err(reason),
        }
    }

    /// The artifact filenames a staging dir is expected to hold.
    pub fn artifact_names(name: &str) -> Vec<String> {
        CertFiles::new(std::path::Path::new(""), name)
            .all()
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect()
    }
}

pub(crate) use crate::{
    config::CertGenConfig,
    generate::{CertFiles, generate},
    install::install,
    verify::verify,
};

pub(crate) use sourcetrait_grammar_lib as lib_grammar;

pub(crate) use std::{
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
    process,
};
