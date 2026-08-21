
pub type TraintrackResult<T> = Result<T, TraintrackError>;

#[derive(Debug, snafu::Snafu)]
pub enum TraintrackError {
}