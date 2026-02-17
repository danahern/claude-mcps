//! Configuration for the openocd-debug MCP server

use std::path::PathBuf;
use clap::Parser;

/// Command line arguments
#[derive(Parser, Debug)]
#[command(name = "openocd-debug")]
#[command(about = "MCP server for embedded debugging via OpenOCD")]
#[command(version)]
pub struct Args {
    /// Path to openocd binary (defaults to searching PATH)
    #[arg(long)]
    pub openocd_path: Option<PathBuf>,

    /// Default serial port for UART console monitor
    #[arg(long)]
    pub serial_port: Option<String>,

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
    /// Path to openocd binary
    pub openocd_path: Option<PathBuf>,
    /// Default serial port for monitor tool
    pub default_serial_port: Option<String>,
}

impl Config {
    pub fn from_args(args: &Args) -> Self {
        Self {
            openocd_path: args.openocd_path.clone(),
            default_serial_port: args.serial_port.clone(),
        }
    }

    /// Find openocd binary path: config, then PATH
    pub fn find_openocd(&self) -> Result<PathBuf, String> {
        if let Some(path) = &self.openocd_path {
            if path.exists() {
                return Ok(path.clone());
            }
            return Err(format!("Configured openocd path does not exist: {}", path.display()));
        }

        // Search PATH
        which("openocd").map_err(|_| {
            "openocd not found. Install via: brew install open-ocd".to_string()
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            openocd_path: None,
            default_serial_port: None,
        }
    }
}

/// Find an executable on PATH (simple which implementation)
fn which(name: &str) -> Result<PathBuf, ()> {
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            let candidate = PathBuf::from(dir).join(name);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_which_finds_ls() {
        // ls should always exist
        assert!(which("ls").is_ok());
    }

    #[test]
    fn test_which_nonexistent() {
        assert!(which("nonexistent_binary_12345").is_err());
    }

    #[test]
    fn test_find_openocd_with_bad_path() {
        let config = Config {
            openocd_path: Some(PathBuf::from("/nonexistent/openocd")),
            default_serial_port: None,
        };
        assert!(config.find_openocd().is_err());
    }
}
