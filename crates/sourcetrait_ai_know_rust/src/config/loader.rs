use crate::*;

/// What: resolve a `Calibration` value from either the embedded default
/// (when `custom` is `None`) or a user-supplied toml file (when
/// `custom` is `Some(path)`).
///
/// Why: replaces the python port's env-var override pattern. The
/// embedded default lives in
/// `crates/sourcetrait_ai_know_rust/assets/calibration.toml`
/// and is bundled via `include_str!` so the binary is self-contained.
/// Users wanting to tune individual knobs supply a full custom toml
/// via the global `-c` CLI flag; partial overrides are not supported
/// in conversion (mirrors the python's all-or-nothing TOML load).
///
/// Where: called once from `crate::run::run` after CLI parse, before
/// dispatching to a subcommand. The resolved value is threaded into
/// any subcommand that consumes calibration.
pub fn load_calibration(custom: Option<&Path>) -> std::result::Result<Calibration, Error> {
    match custom {
        Some(path) => {
            let text = fs::read_to_string(path).map_err(|source| Error::Read {
                path: path.to_path_buf(),
                source,
            })?;
            toml::from_str(&text).map_err(|source| Error::TomlParse {
                path: path.to_path_buf(),
                source,
            })
        }
        None => Ok(Calibration::default()),
    }
}
