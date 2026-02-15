use std::path::PathBuf;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "elf-analysis")]
#[command(about = "MCP server for ELF binary size analysis")]
#[command(version)]
pub struct Args {
    /// Workspace path (used as default for -w flag to size_report)
    #[arg(short, long)]
    pub workspace: Option<PathBuf>,

    /// Path to zephyr/ directory (for size_report script)
    #[arg(long)]
    pub zephyr_base: Option<PathBuf>,

    /// Log level (error, warn, info, debug, trace)
    #[arg(long, default_value = "info")]
    pub log_level: String,

    /// Log file path (defaults to stderr)
    #[arg(long)]
    pub log_file: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub workspace_path: Option<PathBuf>,
    pub zephyr_base: Option<PathBuf>,
}

impl Config {
    pub fn from_args(args: &Args) -> Self {
        let zephyr_base = args.zephyr_base.clone().or_else(|| {
            args.workspace.as_ref().map(|ws| ws.join("zephyr"))
        });

        Self {
            workspace_path: args.workspace.clone(),
            zephyr_base,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            workspace_path: None,
            zephyr_base: None,
        }
    }
}
