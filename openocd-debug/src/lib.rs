//! OpenOCD Debug MCP Server
//!
//! A Model Context Protocol server for embedded debugging via OpenOCD's TCL interface.
//! Connects to OpenOCD over TCP (port 6666) for target control and uses serial for console output.

pub mod config;
pub mod openocd_client;
pub mod tools;

pub use config::{Args, Config};
pub use tools::OpenocdDebugToolHandler;
