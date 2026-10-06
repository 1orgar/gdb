use thiserror::Error;

#[derive(Error, Debug)]
pub enum GdbError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Schema error: {0}")]
    Schema(String),

    #[error("Parser error: {0}")]
    Parser(String),

    #[error("Planner error: {0}")]
    Planner(String),

    #[error("Execution error: {0}")]
    Execution(String),

    #[error("Raft consensus error: {0}")]
    Raft(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("S3 object store error: {0}")]
    S3(String),

    #[error("GPU compute error: {0}")]
    Gpu(String),

    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type GdbResult<T> = Result<T, GdbError>;
