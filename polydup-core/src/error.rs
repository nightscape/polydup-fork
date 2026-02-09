use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PolyDupError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Language parsing error: {0}")]
    Parsing(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Language not supported for file: {0}")]
    LanguageNotSupported(String),

    #[error("Failed to detect language for file: {0}")]
    LanguageDetection(PathBuf),

    #[error("Parallel execution error: {0}")]
    ParallelExecution(String),

    #[error("Ignore rule error: {0}")]
    IgnoreRule(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, PolyDupError>;
