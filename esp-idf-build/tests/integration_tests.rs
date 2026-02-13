//! Integration tests for esp-idf-build MCP server
//!
//! Tests handler creation and config — tool invocation tests are in
//! src/tools/build_tools.rs (they need access to private methods).

use esp_idf_build::{Config, EspIdfBuildToolHandler};

#[test]
fn test_handler_creation() {
    let config = Config::default();
    let _handler = EspIdfBuildToolHandler::new(config);
}

#[test]
fn test_handler_default() {
    let _handler = EspIdfBuildToolHandler::default();
}

#[test]
fn test_config_default_values() {
    let config = Config::default();
    assert!(config.idf_path.is_none());
    assert!(config.projects_dir.is_none());
    assert!(config.default_port.is_none());
}

#[test]
fn test_config_from_args() {
    use clap::Parser;
    use esp_idf_build::config::Args;

    let args = Args::parse_from([
        "esp-idf-build",
        "--idf-path", "/opt/esp-idf",
        "--projects-dir", "/tmp/projects",
        "--port", "/dev/ttyUSB0",
    ]);
    let config = Config::from_args(&args);
    assert_eq!(config.idf_path.unwrap().to_str().unwrap(), "/opt/esp-idf");
    assert_eq!(config.projects_dir.unwrap().to_str().unwrap(), "/tmp/projects");
    assert_eq!(config.default_port.unwrap(), "/dev/ttyUSB0");
}

#[test]
fn test_multiple_handlers() {
    let _h1 = EspIdfBuildToolHandler::default();
    let _h2 = EspIdfBuildToolHandler::new(Config {
        idf_path: Some("/opt/esp-idf".into()),
        projects_dir: None,
        default_port: Some("/dev/ttyUSB0".into()),
    });
}
