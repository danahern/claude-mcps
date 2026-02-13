//! ESP-IDF Build MCP Server

use clap::Parser;
use tracing::{info, error, debug};
use tracing_subscriber::{EnvFilter, fmt};
use rmcp::{ServiceExt, transport::stdio};

use esp_idf_build::{Config, config::Args, tools::EspIdfBuildToolHandler};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    init_logging(&args)?;

    info!("Starting ESP-IDF Build MCP Server v{}", env!("CARGO_PKG_VERSION"));

    let config = Config::from_args(&args);

    let service = EspIdfBuildToolHandler::new(config)
        .serve(stdio()).await.inspect_err(|e| {
            error!("Serving error: {:?}", e);
        })?;

    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use esp_idf_build::config::{Args, Config};

    #[test]
    fn test_args_parsing_defaults() {
        let args = Args::parse_from(["esp-idf-build"]);
        assert!(args.idf_path.is_none());
        assert!(args.projects_dir.is_none());
        assert!(args.port.is_none());
        assert_eq!(args.log_level, "info");
        assert!(args.log_file.is_none());
    }

    #[test]
    fn test_args_parsing_with_options() {
        let args = Args::parse_from([
            "esp-idf-build",
            "--idf-path", "/opt/esp-idf",
            "--projects-dir", "/tmp/projects",
            "--port", "/dev/ttyUSB0",
            "--log-level", "debug",
        ]);
        assert_eq!(args.idf_path.unwrap().to_str().unwrap(), "/opt/esp-idf");
        assert_eq!(args.projects_dir.unwrap().to_str().unwrap(), "/tmp/projects");
        assert_eq!(args.port.unwrap(), "/dev/ttyUSB0");
        assert_eq!(args.log_level, "debug");
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.idf_path.is_none());
        assert!(config.projects_dir.is_none());
        assert!(config.default_port.is_none());
    }

    #[test]
    fn test_config_from_args() {
        let args = Args::parse_from([
            "esp-idf-build",
            "--idf-path", "/opt/esp-idf",
            "--port", "/dev/cu.usbserial-1110",
        ]);
        let config = Config::from_args(&args);
        assert_eq!(config.idf_path.unwrap().to_str().unwrap(), "/opt/esp-idf");
        assert_eq!(config.default_port.unwrap(), "/dev/cu.usbserial-1110");
        assert!(config.projects_dir.is_none());
    }

    #[test]
    fn test_config_from_args_no_options() {
        let args = Args::parse_from(["esp-idf-build"]);
        let config = Config::from_args(&args);
        assert!(config.idf_path.is_none());
        assert!(config.projects_dir.is_none());
        assert!(config.default_port.is_none());
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
