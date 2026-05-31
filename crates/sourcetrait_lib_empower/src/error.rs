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
/// `sourcetrait_lib_empower`. Currently empty (no variants) because
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
/// `nu_sh_mcp` wraps it into `mcp::ErrorData` at the rmcp tool seam.
#[derive(Debug, snafu::Snafu)]
pub enum LibEmpowerError {
}