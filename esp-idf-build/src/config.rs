//! Configuration for the esp-idf-build MCP server

use std::path::PathBuf;
use clap::Parser;

/// Command line arguments
#[derive(Parser, Debug)]
#[command(name = "esp-idf-build")]
#[command(about = "MCP server for building ESP-IDF applications")]
#[command(version)]
pub struct Args {
    /// ESP-IDF path (overrides IDF_PATH env var)
    #[arg(long)]
    pub idf_path: Option<PathBuf>,

    /// Projects directory path
    #[arg(short, long)]
    pub projects_dir: Option<PathBuf>,

    /// Default serial port for flash/monitor
    #[arg(long)]
    pub port: Option<String>,

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
    pub idf_path: Option<PathBuf>,
    pub projects_dir: Option<PathBuf>,
    pub default_port: Option<String>,
}

impl Config {
    pub fn from_args(args: &Args) -> Self {
        Self {
            idf_path: args.idf_path.clone(),
            projects_dir: args.projects_dir.clone(),
            default_port: args.port.clone(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            idf_path: None,
            projects_dir: None,
            default_port: None,
        }
    }
}
