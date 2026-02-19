//! Integration tests for linux-build MCP server

use linux_build::{Config, LinuxBuildToolHandler, Args};
use linux_build::docker_client::{DockerError, ExecResult};
use linux_build::adb_client::{AdbError, AdbDevice};
use clap::Parser;

// --- Handler creation ---

#[test]
fn test_handler_creation() {
    let config = Config::default();
    let _handler = LinuxBuildToolHandler::new(config);
}

#[test]
fn test_handler_with_full_config() {
    let config = Config {
        docker_image: "alif-e7-sdk".to_string(),
        workspace_dir: Some("/tmp/ws".into()),
        default_board_ip: Some("192.168.1.100".to_string()),
        ssh_key: Some("/home/user/.ssh/id_rsa".into()),
        ssh_user: "admin".to_string(),
        default_adb_serial: Some("12345678".to_string()),
    };
    let _handler = LinuxBuildToolHandler::new(config);
}

// --- Config ---

#[test]
fn test_config_defaults() {
    let config = Config::default();
    assert_eq!(config.docker_image, "stm32mp1-sdk");
    assert!(config.workspace_dir.is_none());
    assert!(config.default_board_ip.is_none());
    assert!(config.ssh_key.is_none());
    assert_eq!(config.ssh_user, "root");
    assert!(config.default_adb_serial.is_none());
}

#[test]
fn test_config_from_args() {
    let args = Args::parse_from([
        "linux-build",
        "--docker-image", "custom-sdk",
        "--board-ip", "10.0.0.1",
        "--ssh-user", "admin",
        "--adb-serial", "ABC123",
    ]);
    let config = Config::from_args(&args);
    assert_eq!(config.docker_image, "custom-sdk");
    assert_eq!(config.default_board_ip.unwrap(), "10.0.0.1");
    assert_eq!(config.ssh_user, "admin");
    assert_eq!(config.default_adb_serial.unwrap(), "ABC123");
}

#[test]
fn test_config_from_args_minimal() {
    let args = Args::parse_from(["linux-build"]);
    let config = Config::from_args(&args);
    assert_eq!(config.docker_image, "stm32mp1-sdk");
    assert!(config.default_board_ip.is_none());
}

// --- Error types ---

#[test]
fn test_docker_error_display() {
    let err = DockerError::CommandFailed("test".to_string());
    assert!(err.to_string().contains("test"));

    let err = DockerError::ContainerStartFailed("port conflict".to_string());
    assert!(err.to_string().contains("port conflict"));

    let err = DockerError::CopyFailed("no such file".to_string());
    assert!(err.to_string().contains("no such file"));

    let err = DockerError::DeployFailed("ssh timeout".to_string());
    assert!(err.to_string().contains("ssh timeout"));
}

#[test]
fn test_adb_error_display() {
    let err = AdbError::CommandFailed("not found".to_string());
    assert!(err.to_string().contains("not found"));

    let err = AdbError::PushFailed("no device".to_string());
    assert!(err.to_string().contains("no device"));

    let err = AdbError::PullFailed("not found".to_string());
    assert!(err.to_string().contains("not found"));
}

// --- Data types ---

#[test]
fn test_adb_device_struct() {
    let device = AdbDevice {
        serial: "12345678".to_string(),
        state: "device".to_string(),
    };
    assert_eq!(device.serial, "12345678");
    assert_eq!(device.state, "device");
}

#[test]
fn test_exec_result_success() {
    let result = ExecResult {
        success: true,
        stdout: "output".to_string(),
        stderr: String::new(),
        exit_code: 0,
    };
    assert!(result.success);
    assert_eq!(result.exit_code, 0);
}

#[test]
fn test_exec_result_failure() {
    let result = ExecResult {
        success: false,
        stdout: String::new(),
        stderr: "error occurred".to_string(),
        exit_code: 1,
    };
    assert!(!result.success);
    assert_eq!(result.exit_code, 1);
    assert!(result.stderr.contains("error"));
}
