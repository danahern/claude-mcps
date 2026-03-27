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

    /// Applications directory relative to workspace (default: "firmware/apps")
    #[arg(long)]
    pub apps_dir: Option<String>,

    /// Run builds inside a Docker container (docker exec mode)
    #[arg(long, default_value = "false")]
    pub docker: bool,

    /// Docker container name to exec into (default: "zephyr-build")
    #[arg(long, default_value = "zephyr-build")]
    pub docker_container: String,

    /// Docker image to use when starting the container (default: "zephyr-builder")
    #[arg(long, default_value = "zephyr-builder")]
    pub docker_image: String,

    /// Host path to Zephyr SDK (mounted into container at /opt/zephyr-sdk)
    #[arg(long)]
    pub sdk_path: Option<PathBuf>,

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
    pub docker: bool,
    pub docker_container: String,
    pub docker_image: String,
    pub sdk_path: Option<PathBuf>,
}

impl Config {
    pub fn from_args(args: &Args) -> Self {
        Self {
            workspace_path: args.workspace.clone(),
            apps_dir: args.apps_dir.clone().unwrap_or_else(|| "firmware/apps".to_string()),
            docker: args.docker,
            docker_container: args.docker_container.clone(),
            docker_image: args.docker_image.clone(),
            sdk_path: args.sdk_path.clone(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            workspace_path: None,
            apps_dir: "firmware/apps".to_string(),
            docker: false,
            docker_container: "zephyr-build".to_string(),
            docker_image: "zephyr-builder".to_string(),
            sdk_path: None,
        }
    }
}
