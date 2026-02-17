//! OpenOCD Debug MCP Server — Main Entry Point

use clap::Parser;
use tracing::{info, error, debug};
use tracing_subscriber::{EnvFilter, fmt};
use rmcp::{ServiceExt, transport::stdio};

use openocd_debug::{Args, Config, OpenocdDebugToolHandler};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    init_logging(&args)?;

    info!("Starting OpenOCD Debug MCP Server v{}", env!("CARGO_PKG_VERSION"));

    let config = Config::from_args(&args);

    let service = OpenocdDebugToolHandler::new(config)
        .serve(stdio()).await.inspect_err(|e| {
            error!("Serving error: {:?}", e);
        })?;

    service.waiting().await?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use clap::Parser;
    use openocd_debug::config::{Args, Config};

    #[test]
    fn test_args_parsing_defaults() {
        let args = Args::parse_from(["openocd-debug"]);
        assert!(args.openocd_path.is_none());
        assert!(args.serial_port.is_none());
        assert_eq!(args.log_level, "info");
    }

    #[test]
    fn test_args_parsing_with_options() {
        let args = Args::parse_from([
            "openocd-debug",
            "--openocd-path", "/usr/local/bin/openocd",
            "--serial-port", "/dev/ttyACM0",
            "--log-level", "debug",
        ]);
        assert_eq!(args.openocd_path.unwrap().to_str().unwrap(), "/usr/local/bin/openocd");
        assert_eq!(args.serial_port.unwrap(), "/dev/ttyACM0");
        assert_eq!(args.log_level, "debug");
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.openocd_path.is_none());
        assert!(config.default_serial_port.is_none());
    }

    #[test]
    fn test_config_from_args() {
        let args = Args::parse_from([
            "openocd-debug",
            "--openocd-path", "/usr/local/bin/openocd",
            "--serial-port", "/dev/ttyACM0",
        ]);
        let config = Config::from_args(&args);
        assert_eq!(config.openocd_path.unwrap().to_str().unwrap(), "/usr/local/bin/openocd");
        assert_eq!(config.default_serial_port.unwrap(), "/dev/ttyACM0");
    }
}
