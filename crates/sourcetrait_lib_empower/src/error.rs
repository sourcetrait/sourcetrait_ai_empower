//use crate::*;

pub type LibEmpowerResult<T> = Result<T, LibEmpowerError>;

#[derive(Debug, snafu::Snafu)]
pub enum LibEmpowerError {
}