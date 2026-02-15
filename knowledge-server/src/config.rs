use std::path::PathBuf;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "knowledge-server")]
#[command(about = "MCP server for structured knowledge management")]
#[command(version)]
pub struct Args {
    /// Workspace root path (contains knowledge/, plans/, .claude/)
    #[arg(short, long)]
    pub workspace: Option<PathBuf>,

    /// Log level (error, warn, info, debug, trace)
    #[arg(long, default_value = "info")]
    pub log_level: String,

    /// Log file path (defaults to stderr)
    #[arg(long)]
    pub log_file: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub workspace_path: PathBuf,
}

impl Config {
    pub fn from_args(args: &Args) -> Self {
        let workspace_path = args.workspace.clone()
            .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

        Self { workspace_path }
    }

    /// Path to knowledge/items/ directory
    pub fn items_dir(&self) -> PathBuf {
        self.workspace_path.join("knowledge").join("items")
    }

    /// Path to knowledge/boards/ directory
    pub fn boards_dir(&self) -> PathBuf {
        self.workspace_path.join("knowledge").join("boards")
    }

    /// Path to .cache/ directory for SQLite index
    pub fn cache_dir(&self) -> PathBuf {
        self.workspace_path.join(".cache")
    }

    /// Path to .claude/rules/ directory
    pub fn rules_dir(&self) -> PathBuf {
        self.workspace_path.join(".claude").join("rules")
    }
}
