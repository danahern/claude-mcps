use clap::Parser;
use tracing::{info, error, debug};
use tracing_subscriber::{EnvFilter, fmt};
use rmcp::{ServiceExt, transport::stdio};

use knowledge_server::{Config, config::Args, tools::KnowledgeToolHandler};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    init_logging(&args)?;

    info!("Starting Knowledge Server MCP v{}", env!("CARGO_PKG_VERSION"));

    let config = Config::from_args(&args);
    let handler = KnowledgeToolHandler::new(config)?;

    let service = handler
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
