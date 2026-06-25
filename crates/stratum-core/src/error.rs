use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransformError {
    #[error("skipped")]
    Skipped,
    #[error("invalid input")]
    InvalidInput,
    #[error("internal error")]
    Internal,
}
