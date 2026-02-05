//! Configuration management for the zephyr-build MCP server

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use clap::Parser;
use crate::error::{BuildError, Result};

/// Command line arguments
#[derive(Parser, Debug)]
#[command(name = "zephyr-build")]
#[command(about = "A Model Context Protocol server for building Zephyr applications")]
#[command(version)]
pub struct Args {
    /// Path to configuration file
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Zephyr workspace path
    #[arg(short, long)]
    pub workspace: Option<PathBuf>,

    /// Log level (error, warn, info, debug, trace)
    #[arg(long, default_value = "info")]
    pub log_level: String,

    /// Log file path
    #[arg(long)]
    pub log_file: Option<PathBuf>,

    /// Generate default configuration file
    #[arg(long)]
    pub generate_config: bool,

    /// Validate configuration and exit
    #[arg(long)]
    pub validate_config: bool,

    /// Show current configuration and exit
    #[arg(long)]
    pub show_config: bool,
}

/// Main configuration structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub workspace: WorkspaceConfig,
    pub build: BuildConfig,
    pub logging: LoggingConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            workspace: WorkspaceConfig::default(),
            build: BuildConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from file or create default
    pub fn load(config_path: Option<&PathBuf>) -> Result<Self> {
        if let Some(path) = config_path {
            let content = std::fs::read_to_string(path)
                .map_err(|e| BuildError::InvalidConfig(format!("Failed to read config file: {}", e)))?;
            let config: Config = toml::from_str(&content)
                .map_err(|e| BuildError::InvalidConfig(format!("Invalid TOML syntax: {}", e)))?;
            config.validate()?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    /// Merge command line arguments into configuration
    pub fn merge_args(&mut self, args: &Args) {
        if let Some(workspace) = &args.workspace {
            self.workspace.path = Some(workspace.clone());
        }
        self.logging.level = args.log_level.clone();
        self.logging.file = args.log_file.clone();
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }

    /// Generate TOML configuration string
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self)
            .map_err(|e| BuildError::InvalidConfig(format!("Failed to serialize config: {}", e)))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspaceConfig {
    /// Path to the Zephyr workspace (contains .west/ and zephyr-apps/)
    pub path: Option<PathBuf>,
    /// Subdirectory containing applications (relative to workspace)
    pub apps_dir: String,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            path: None,
            apps_dir: "zephyr-apps/apps".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BuildConfig {
    /// Default to pristine builds
    pub default_pristine: bool,
    /// Build timeout in seconds
    pub timeout_seconds: u64,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            default_pristine: false,
            timeout_seconds: 600, // 10 minutes
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoggingConfig {
    pub level: String,
    pub file: Option<PathBuf>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            file: None,
        }
    }
}
