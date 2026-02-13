//! Error types for the esp-idf-build MCP server

use thiserror::Error;

/// Main error type for the esp-idf-build MCP server
#[derive(Error, Debug)]
pub enum BuildError {
    #[error("IDF_PATH not found: {0}")]
    IdfPathNotFound(String),

    #[error("ESP-IDF environment setup failed: {0}")]
    EnvSetupFailed(String),

    #[error("Project not found: {0}")]
    ProjectNotFound(String),

    #[error("Build failed: {0}")]
    BuildFailed(String),

    #[error("Build not found: {0}")]
    BuildNotFound(String),

    #[error("Flash failed: {0}")]
    FlashFailed(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    InternalError(String),
}

/// Result type alias for convenience
pub type Result<T> = std::result::Result<T, BuildError>;
