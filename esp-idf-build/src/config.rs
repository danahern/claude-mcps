//! Configuration management for the esp-idf-build MCP server

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use clap::Parser;
use crate::error::{BuildError, Result};

/// Command line arguments
#[derive(Parser, Debug)]
#[command(name = "esp-idf-build")]
#[command(about = "A Model Context Protocol server for building ESP-IDF applications")]
#[command(version)]
pub struct Args {
    /// Path to configuration file
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// ESP-IDF path (overrides IDF_PATH)
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
    pub idf: IdfConfig,
    pub build: BuildConfig,
    pub logging: LoggingConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            idf: IdfConfig::default(),
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
        if let Some(idf_path) = &args.idf_path {
            self.idf.idf_path = Some(idf_path.clone());
        }
        if let Some(projects_dir) = &args.projects_dir {
            self.idf.projects_dir = Some(projects_dir.clone());
        }
        if let Some(port) = &args.port {
            self.idf.default_port = Some(port.clone());
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
pub struct IdfConfig {
    /// Path to ESP-IDF (overrides IDF_PATH env var)
    pub idf_path: Option<PathBuf>,
    /// Directory containing ESP-IDF projects
    pub projects_dir: Option<PathBuf>,
    /// Default serial port for flash/monitor
    pub default_port: Option<String>,
}

impl Default for IdfConfig {
    fn default() -> Self {
        Self {
            idf_path: None,
            projects_dir: None,
            default_port: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BuildConfig {
    /// Build timeout in seconds
    pub timeout_seconds: u64,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 600,
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
