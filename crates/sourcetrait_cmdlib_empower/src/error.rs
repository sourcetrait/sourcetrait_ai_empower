//use crate::*;

pub type CmdEmpowerResult<T> = Result<T, CmdEmpowerError>;

#[derive(Debug, snafu::Snafu)]
pub enum CmdEmpowerError {
}
