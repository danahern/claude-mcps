//! Configuration for the linux-build MCP server

use std::path::PathBuf;
use clap::Parser;

/// Command line arguments
#[derive(Parser, Debug)]
#[command(name = "linux-build")]
#[command(about = "MCP server for Linux cross-compilation via Docker")]
#[command(version)]
pub struct Args {
    /// Docker image name for the build environment
    #[arg(long, default_value = "stm32mp1-sdk")]
    pub docker_image: String,

    /// Host directory to mount as workspace in container
    #[arg(long)]
    pub workspace_dir: Option<PathBuf>,

    /// Default board IP address for SSH deployment
    #[arg(long)]
    pub board_ip: Option<String>,

    /// SSH key path for board deployment
    #[arg(long)]
    pub ssh_key: Option<PathBuf>,

    /// SSH user for board deployment (default: root)
    #[arg(long, default_value = "root")]
    pub ssh_user: String,

    /// Log level (error, warn, info, debug, trace)
    #[arg(long, default_value = "info")]
    pub log_level: String,

    /// Log file path (defaults to stderr)
    #[arg(long)]
    pub log_file: Option<PathBuf>,
}

/// Runtime configuration
#[derive(Debug, Clone)]
pub struct Config {
    pub docker_image: String,
    pub workspace_dir: Option<PathBuf>,
    pub default_board_ip: Option<String>,
    pub ssh_key: Option<PathBuf>,
    pub ssh_user: String,
}

impl Config {
    pub fn from_args(args: &Args) -> Self {
        Self {
            docker_image: args.docker_image.clone(),
            workspace_dir: args.workspace_dir.clone(),
            default_board_ip: args.board_ip.clone(),
            ssh_key: args.ssh_key.clone(),
            ssh_user: args.ssh_user.clone(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            docker_image: "stm32mp1-sdk".to_string(),
            workspace_dir: None,
            default_board_ip: None,
            ssh_key: None,
            ssh_user: "root".to_string(),
        }
    }
}
