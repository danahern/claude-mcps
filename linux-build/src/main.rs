//! Linux Build MCP Server — Main Entry Point

use clap::Parser;
use tracing::{info, error, debug};
use tracing_subscriber::{EnvFilter, fmt};
use rmcp::{ServiceExt, transport::stdio};

use linux_build::{Args, Config, LinuxBuildToolHandler};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    init_logging(&args)?;

    info!("Starting Linux Build MCP Server v{}", env!("CARGO_PKG_VERSION"));

    let config = Config::from_args(&args);

    let service = LinuxBuildToolHandler::new(config)
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
    use linux_build::config::{Args, Config};

    #[test]
    fn test_args_parsing_defaults() {
        let args = Args::parse_from(["linux-build"]);
        assert_eq!(args.docker_image, "stm32mp1-sdk");
        assert!(args.workspace_dir.is_none());
        assert!(args.board_ip.is_none());
        assert_eq!(args.ssh_user, "root");
    }

    #[test]
    fn test_args_parsing_with_options() {
        let args = Args::parse_from([
            "linux-build",
            "--docker-image", "my-sdk:latest",
            "--board-ip", "192.168.1.100",
            "--ssh-user", "admin",
        ]);
        assert_eq!(args.docker_image, "my-sdk:latest");
        assert_eq!(args.board_ip.unwrap(), "192.168.1.100");
        assert_eq!(args.ssh_user, "admin");
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.docker_image, "stm32mp1-sdk");
        assert!(config.default_board_ip.is_none());
        assert_eq!(config.ssh_user, "root");
    }

    #[test]
    fn test_config_from_args() {
        let args = Args::parse_from([
            "linux-build",
            "--board-ip", "10.0.0.1",
            "--ssh-key", "/home/user/.ssh/id_rsa",
        ]);
        let config = Config::from_args(&args);
        assert_eq!(config.default_board_ip.unwrap(), "10.0.0.1");
        assert!(config.ssh_key.is_some());
    }
}
