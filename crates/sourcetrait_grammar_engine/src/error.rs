pub type GrammarEngineResult<T> = Result<T, GrammarEngineError>;

#[derive(Debug, snafu::Snafu)]
pub struct GrammarEngineError {
}