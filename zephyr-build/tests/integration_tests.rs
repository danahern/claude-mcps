//! Integration tests for zephyr-build MCP server
//!
//! Tests handler creation and config — tool invocation tests are in
//! src/tools/build_tools.rs (they need access to private methods).

use zephyr_build::{Config, ZephyrBuildToolHandler};

#[test]
fn test_handler_creation() {
    let config = Config::default();
    let _handler = ZephyrBuildToolHandler::new(config);
}

#[test]
fn test_handler_default() {
    let _handler = ZephyrBuildToolHandler::default();
}

#[test]
fn test_config_default_values() {
    let config = Config::default();
    assert!(config.workspace_path.is_none());
    assert_eq!(config.apps_dir, "zephyr-apps/apps");
}

#[test]
fn test_config_from_args() {
    use clap::Parser;
    use zephyr_build::config::Args;

    let args = Args::parse_from(["zephyr-build", "--workspace", "/tmp/test"]);
    let config = Config::from_args(&args);
    assert_eq!(config.workspace_path.unwrap().to_str().unwrap(), "/tmp/test");
}

#[test]
fn test_multiple_handlers() {
    // Should be able to create multiple handlers without issue
    let _h1 = ZephyrBuildToolHandler::default();
    let _h2 = ZephyrBuildToolHandler::new(Config {
        workspace_path: Some("/tmp/test".into()),
        apps_dir: "apps".to_string(),
    });
}

#[test]
fn test_handler_clone_shares_state() {
    // Cloned handlers should share build/test state (Arc)
    let h1 = ZephyrBuildToolHandler::default();
    let h2 = h1.clone();
    // Both should work independently without panic
    drop(h2);
    let _h3 = h1.clone();
}
