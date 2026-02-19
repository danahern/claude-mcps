//! Integration tests for elf-analysis MCP server

use elf_analysis::{Config, ElfAnalysisToolHandler};
use elf_analysis::config::Args;
use clap::Parser;

#[test]
fn test_handler_creation() {
    let config = Config::default();
    let _handler = ElfAnalysisToolHandler::new(config);
}

#[test]
fn test_handler_with_workspace() {
    let config = Config {
        workspace_path: Some("/tmp/test-ws".into()),
        zephyr_base: Some("/tmp/test-ws/zephyr".into()),
    };
    let _handler = ElfAnalysisToolHandler::new(config);
}

#[test]
fn test_config_default() {
    let config = Config::default();
    assert!(config.workspace_path.is_none());
    assert!(config.zephyr_base.is_none());
}

#[test]
fn test_config_from_args_defaults() {
    let args = Args::parse_from(["elf-analysis"]);
    let config = Config::from_args(&args);
    assert!(config.workspace_path.is_none());
    assert!(config.zephyr_base.is_none());
}

#[test]
fn test_config_derives_zephyr_base_from_workspace() {
    let args = Args::parse_from(["elf-analysis", "--workspace", "/tmp/ws"]);
    let config = Config::from_args(&args);
    assert_eq!(config.workspace_path.unwrap().to_str().unwrap(), "/tmp/ws");
    assert_eq!(config.zephyr_base.unwrap().to_str().unwrap(), "/tmp/ws/zephyr");
}

#[test]
fn test_config_explicit_zephyr_base_overrides_derived() {
    let args = Args::parse_from([
        "elf-analysis", "--workspace", "/tmp/ws", "--zephyr-base", "/opt/zephyr"
    ]);
    let config = Config::from_args(&args);
    assert_eq!(config.zephyr_base.unwrap().to_str().unwrap(), "/opt/zephyr");
}

#[test]
fn test_multiple_handlers() {
    let _h1 = ElfAnalysisToolHandler::new(Config::default());
    let _h2 = ElfAnalysisToolHandler::new(Config {
        workspace_path: Some("/tmp/a".into()),
        zephyr_base: Some("/tmp/a/zephyr".into()),
    });
}
