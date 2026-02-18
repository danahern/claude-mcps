//! Linux Build MCP Server
//!
//! Thin Docker wrapper for cross-compiling Linux applications.
//! Manages container lifecycle, builds, artifact collection, and SSH deployment.

pub mod adb_client;
pub mod config;
pub mod docker_client;
pub mod tools;

pub use config::{Args, Config};
pub use tools::LinuxBuildToolHandler;
