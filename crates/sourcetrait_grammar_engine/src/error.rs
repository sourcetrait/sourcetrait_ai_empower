use crate::*;

pub type GrammarEngineResult<T> = Result<T, GrammarEngineError>;

#[derive(Debug, snafu::Snafu)]
pub enum GrammarEngineError {
    EngineParameter {
        parameter: EngineParameterKind,
    },
}