//! ESP-IDF Build MCP Server
//!
//! A Model Context Protocol server for building ESP-IDF applications.
//! Provides AI assistants with build, flash, and monitor capabilities for ESP-IDF projects.

pub mod config;
pub mod error;
pub mod tools;

pub use error::{BuildError, Result};
pub use config::Config;
pub use tools::EspIdfBuildToolHandler;
