//! Zephyr Build MCP Server

use clap::Parser;
use tracing::{info, error, debug};
use tracing_subscriber::{EnvFilter, fmt};
use rmcp::{ServiceExt, transport::stdio};

use zephyr_build::{Config, config::Args, tools::ZephyrBuildToolHandler};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    init_logging(&args)?;

    info!("Starting Zephyr Build MCP Server v{}", env!("CARGO_PKG_VERSION"));

    let config = Config::from_args(&args);

    let service = ZephyrBuildToolHandler::new(config)
        .serve(stdio()).await.inspect_err(|e| {
            error!("Serving error: {:?}", e);
        })?;

    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use zephyr_build::config::{Args, Config};

    #[test]
    fn test_args_parsing_defaults() {
        let args = Args::parse_from(["zephyr-build"]);
        assert!(args.workspace.is_none());
        assert_eq!(args.log_level, "info");
        assert!(args.log_file.is_none());
    }

    #[test]
    fn test_args_parsing_with_workspace() {
        let args = Args::parse_from(["zephyr-build", "--workspace", "/tmp/ws", "--log-level", "debug"]);
        assert_eq!(args.workspace.unwrap().to_str().unwrap(), "/tmp/ws");
        assert_eq!(args.log_level, "debug");
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.workspace_path.is_none());
        assert_eq!(config.apps_dir, "zephyr-apps/apps");
    }

    #[test]
    fn test_config_from_args() {
        let args = Args::parse_from(["zephyr-build", "--workspace", "/tmp/ws"]);
        let config = Config::from_args(&args);
        assert_eq!(config.workspace_path.unwrap().to_str().unwrap(), "/tmp/ws");
        assert_eq!(config.apps_dir, "zephyr-apps/apps");
    }

    #[test]
    fn test_config_from_args_no_workspace() {
        let args = Args::parse_from(["zephyr-build"]);
        let config = Config::from_args(&args);
        assert!(config.workspace_path.is_none());
    }
}

fn init_logging(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&args.log_level));

    let subscriber = fmt::Subscriber::builder()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(false)
        .with_line_number(false);

    if let Some(log_file) = &args.log_file {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)?;
        subscriber.with_writer(file).init();
    } else {
        subscriber.with_writer(std::io::stderr).init();
    }

    debug!("Logging initialized with level: {}", args.log_level);
    Ok(())
}
