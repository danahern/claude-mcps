//! Configuration for the zephyr-build MCP server

use std::path::PathBuf;
use clap::Parser;

/// Command line arguments
#[derive(Parser, Debug)]
#[command(name = "zephyr-build")]
#[command(about = "MCP server for building Zephyr applications")]
#[command(version)]
pub struct Args {
    /// Zephyr workspace path
    #[arg(short, long)]
    pub workspace: Option<PathBuf>,

    /// Log level (error, warn, info, debug, trace)
    #[arg(long, default_value = "info")]
    pub log_level: String,

    /// Log file path (defaults to stderr)
    #[arg(long)]
    pub log_file: Option<PathBuf>,
}

/// Runtime configuration derived from CLI args
#[derive(Debug, Clone)]
pub struct Config {
    pub workspace_path: Option<PathBuf>,
    pub apps_dir: String,
}

impl Config {
    pub fn from_args(args: &Args) -> Self {
        Self {
            workspace_path: args.workspace.clone(),
            apps_dir: "zephyr-apps/apps".to_string(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            workspace_path: None,
            apps_dir: "zephyr-apps/apps".to_string(),
        }
    }
}
