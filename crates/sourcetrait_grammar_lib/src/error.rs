/// What: result alias parameterized only by the success type, with
/// `LibGrammarError` baked in as the error variant.
///
/// Why: lets sister crates write `LibGrammarResult<T>` without re-spelling the
/// error type at every call site; mirrors the `std::io::Result<T>` convention.
/// The error half is an empty reserved shell today - every remaining
/// lib_grammar surface (the base62 helpers) is infallible.
///
/// Where: returned (eventually) by any future fallible lib_grammar helper. No
/// current function surfaces it.
pub type LibGrammarResult<T> = Result<T, LibGrammarError>;

/// What: the crate's canonical error enum, currently with no variants - every
/// remaining surface (base62) is infallible.
///
/// Why: locks `LibGrammarError` in as the canonical error name and derives
/// `snafu::Snafu` so future variants gain Display, `.context()`, and
/// source-chain ergonomics without per-variant boilerplate. The markdown
/// surface (the lone prior variant) moved into
/// `sourcetrait_nu_plugin_grimoire` when lib_grammar dissolved to its
/// shared remainder (base62 + consts).
///
/// Where: the error half of `LibGrammarResult<T>`.
#[derive(Debug, snafu::Snafu)]
pub enum LibGrammarError {}
