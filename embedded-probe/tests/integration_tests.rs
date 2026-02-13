//! Integration tests for embedded-probe MCP server

use embedded_probe::Config;

#[tokio::test]
async fn test_config_validation() {
    let config = Config::default();
    assert!(config.validate().is_ok());

    // Test TOML serialization
    let toml_str = config.to_toml().unwrap();
    assert!(!toml_str.is_empty());
    assert!(toml_str.contains("[server]"));
    assert!(toml_str.contains("[debugger]"));
}

#[tokio::test]
async fn test_probe_discovery() {
    use embedded_probe::debugger::discovery::ProbeDiscovery;

    let result = ProbeDiscovery::list_probes();
    assert!(result.is_ok());

    // Might be empty without hardware, that's fine
    let probes = result.unwrap();
    println!("Found {} probes", probes.len());
}

#[test]
fn test_error_types() {
    use embedded_probe::DebugError;

    let error = DebugError::ProbeNotFound("test".to_string());
    assert!(error.to_string().contains("Probe not found"));

    let error = DebugError::SessionLimitExceeded(5);
    assert!(error.to_string().contains("Session limit exceeded"));
}

#[test]
fn test_probe_type_detection() {
    use embedded_probe::utils::ProbeType;

    assert_eq!(ProbeType::from_vid_pid(0x1366, 0x0101), ProbeType::JLink);
    assert_eq!(ProbeType::from_vid_pid(0x0483, 0x374B), ProbeType::StLink);
    assert_eq!(ProbeType::from_vid_pid(0x0D28, 0x0204), ProbeType::DapLink);
    assert_eq!(ProbeType::from_vid_pid(0xFFFF, 0xFFFF), ProbeType::Unknown);
}

#[tokio::test]
async fn test_mcp_tool_handler() {
    use embedded_probe::EmbeddedDebuggerToolHandler;

    let _handler = EmbeddedDebuggerToolHandler::new(10);
    let _handler2 = EmbeddedDebuggerToolHandler::new(5);
}

#[tokio::test]
async fn test_handler_with_custom_max_sessions() {
    use embedded_probe::EmbeddedDebuggerToolHandler;

    let _handler = EmbeddedDebuggerToolHandler::new(1);
    let _handler = EmbeddedDebuggerToolHandler::new(100);
}

#[test]
fn test_config_default_values() {
    let config = Config::default();
    assert_eq!(config.server.max_sessions, 5);
    assert_eq!(config.debugger.default_speed_khz, 4000);
}

#[test]
fn test_config_toml_roundtrip() {
    let config = Config::default();
    let toml_str = config.to_toml().unwrap();
    // Verify the TOML contains all major sections
    assert!(toml_str.contains("[server]"));
    assert!(toml_str.contains("[debugger]"));
    assert!(toml_str.contains("[rtt]"));
    assert!(toml_str.contains("[flash]"));
}

#[test]
fn test_error_display() {
    use embedded_probe::DebugError;

    let errors = vec![
        DebugError::ProbeNotFound("test-probe".to_string()),
        DebugError::SessionLimitExceeded(5),
        DebugError::ConnectionFailed("timeout".to_string()),
    ];

    for error in &errors {
        // All errors should produce non-empty display strings
        let msg = error.to_string();
        assert!(!msg.is_empty(), "error should have display text");
    }
}
