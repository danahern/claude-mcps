//! Zephyr Build MCP Server
//!
//! A Model Context Protocol server for building Zephyr RTOS applications.
//! Provides AI assistants with build capabilities for Zephyr projects.

pub mod config;
pub mod error;
pub mod tools;

pub use error::{BuildError, Result};
pub use config::Config;
pub use tools::ZephyrBuildToolHandler;
