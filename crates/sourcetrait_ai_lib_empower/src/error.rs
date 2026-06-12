//use crate::*;

/// What: result alias parameterized only by the success type, with
/// `LibEmpowerError` baked in as the error variant.
///
/// Why: lets sister crates write `LibEmpowerResult<Nonce>` without
/// re-spelling the error type at every call site; mirrors the
/// convention `std::io::Result<T>` set.
///
/// Where: every fallible public function in this crate returns
/// `LibEmpowerResult<T>` so the workspace-wide error story stays
/// uniform. Currently no functions surface this since the existing
/// public surface (Nonce, RerunHash, base62 helpers) is infallible.
pub type LibEmpowerResult<T> = Result<T, LibEmpowerError>;

/// What: enum of error variants for fallible operations in
/// `sourcetrait_ai_lib_empower`. Currently empty (no variants) because
/// every public operation is infallible.
///
/// Why: derives `snafu::Snafu` so future variants gain Display,
/// `.context()`, and source-chain ergonomics without per-variant
/// boilerplate. The empty stub exists to lock the workspace into
/// `LibEmpowerError` as the canonical error name before any fallible
/// helper lands.
///
/// Where: returned (eventually) as the error half of
/// `LibEmpowerResult<T>` from any public lib_empower function that
/// can fail. Sister crates match on its variants; downstream
/// `sourcetrait_ai_nushell_mcp` wraps it into `mcp::ErrorData` at the rmcp tool seam.
#[derive(Debug, snafu::Snafu)]
pub enum LibEmpowerError {
    /// Markdown surface errors bubble into the crate error even
    /// where no caller matches them directly - nested calls inside
    /// the crate `?` a `MarkdownResult` straight into a
    /// `LibEmpowerResult` through the generated `From`.
    #[snafu(transparent)]
    Markdown { source: MarkdownError },
}

/// What: result alias for the markdown structural-query surface,
/// with `MarkdownError` baked in as the error variant.
///
/// Why: same `std::io::Result<T>` convention as
/// `LibEmpowerResult`; callers write `MarkdownResult<Vec<...>>`
/// without re-spelling the error type.
///
/// Where: returned by `markdown::find` (re-exported as `md::find`);
/// consumed by `nu_plugin_empower`'s md commands through the
/// crate's `md` module.
pub type MarkdownResult<T> = Result<T, MarkdownError>;

/// What: error variants for the markdown structural-query surface -
/// file-read failures and invalid regex patterns.
///
/// Why: errors live in `error.rs` regardless of the module they
/// represent (the_user's lib convention); snafu derives Display +
/// `.context()` selectors consumed at the `markdown` call sites.
///
/// Where: the error half of `MarkdownResult<T>`; matched by sister
/// crates (the plugin's md commands map it onto nushell errors).
#[derive(Debug, snafu::Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum MarkdownError {
    #[snafu(display("could not read markdown file: {}", path.display()))]
    ReadFile {
        path: std::path::PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("invalid regex pattern: {pattern}"))]
    InvalidPattern {
        pattern: String,
        source: regex::Error,
    },
}