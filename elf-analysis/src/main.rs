use clap::Parser;
use tracing::{info, error, debug};
use tracing_subscriber::{EnvFilter, fmt};
use rmcp::{ServiceExt, transport::stdio};

use elf_analysis::{Config, config::Args, tools::ElfAnalysisToolHandler};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    init_logging(&args)?;

    info!("Starting ELF Analysis MCP Server v{}", env!("CARGO_PKG_VERSION"));

    let config = Config::from_args(&args);

    let service = ElfAnalysisToolHandler::new(config)
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
    use elf_analysis::config::{Args, Config};

    #[test]
    fn test_args_parsing_defaults() {
        let args = Args::parse_from(["elf-analysis"]);
        assert!(args.workspace.is_none());
        assert!(args.zephyr_base.is_none());
        assert_eq!(args.log_level, "info");
        assert!(args.log_file.is_none());
    }

    #[test]
    fn test_args_parsing_with_workspace() {
        let args = Args::parse_from(["elf-analysis", "--workspace", "/tmp/ws"]);
        assert_eq!(args.workspace.unwrap().to_str().unwrap(), "/tmp/ws");
    }

    #[test]
    fn test_config_from_args_derives_zephyr_base() {
        let args = Args::parse_from(["elf-analysis", "--workspace", "/tmp/ws"]);
        let config = Config::from_args(&args);
        assert_eq!(config.zephyr_base.unwrap().to_str().unwrap(), "/tmp/ws/zephyr");
    }

    #[test]
    fn test_config_from_args_explicit_zephyr_base() {
        let args = Args::parse_from([
            "elf-analysis", "--workspace", "/tmp/ws", "--zephyr-base", "/opt/zephyr"
        ]);
        let config = Config::from_args(&args);
        assert_eq!(config.zephyr_base.unwrap().to_str().unwrap(), "/opt/zephyr");
    }

    #[test]
    fn test_config_from_args_no_workspace() {
        let args = Args::parse_from(["elf-analysis"]);
        let config = Config::from_args(&args);
        assert!(config.workspace_path.is_none());
        assert!(config.zephyr_base.is_none());
    }
}
