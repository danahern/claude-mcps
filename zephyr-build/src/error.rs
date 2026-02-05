//! Error types for the zephyr-build MCP server

use thiserror::Error;

/// Main error type for the zephyr-build MCP server
#[derive(Error, Debug)]
pub enum BuildError {
    #[error("Workspace not found: {0}")]
    WorkspaceNotFound(String),

    #[error("Application not found: {0}")]
    AppNotFound(String),

    #[error("Build failed: {0}")]
    BuildFailed(String),

    #[error("Build not found: {0}")]
    BuildNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("West command failed: {0}")]
    WestError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    InternalError(String),
}

/// Result type alias for convenience
pub type Result<T> = std::result::Result<T, BuildError>;
