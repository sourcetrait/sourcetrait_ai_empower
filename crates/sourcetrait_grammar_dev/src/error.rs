use crate::*;

pub type DocResult<T> = Result<T, DocError>;
pub type DocNuResult = Result<nu::Value, DocError>;

#[derive(Debug, Clone, Copy, snafu::Snafu)]
pub enum DocError {
}