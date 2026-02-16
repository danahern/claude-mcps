//! Type definitions for ESP-IDF build MCP tools

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

// ============================================================================
// list_projects
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListProjectsArgs {
    /// Override default projects directory
    #[serde(default)]
    pub projects_dir: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListProjectsResult {
    pub projects: Vec<ProjectInfo>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ProjectInfo {
    /// Project name (directory name)
    pub name: String,
    /// Full path to project
    pub path: String,
    /// Whether a build directory exists
    pub has_build: bool,
    /// Target from sdkconfig (if available)
    pub target: Option<String>,
}

// ============================================================================
// list_targets
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListTargetsArgs {}

#[derive(Debug, Serialize)]
pub struct ListTargetsResult {
    pub targets: Vec<TargetInfo>,
}

#[derive(Debug, Serialize, Clone)]
pub struct TargetInfo {
    /// Target identifier (e.g., "esp32p4")
    pub name: String,
    /// Architecture (e.g., "riscv", "xtensa")
    pub arch: String,
    /// Description
    pub description: String,
}

// ============================================================================
// set_target
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetTargetArgs {
    /// Project name or path
    pub project: String,
    /// Target chip (e.g., "esp32p4", "esp32s3")
    pub target: String,
    /// Override default projects directory
    #[serde(default)]
    pub projects_dir: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SetTargetResult {
    pub success: bool,
    pub output: String,
}

// ============================================================================
// build
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BuildArgs {
    /// Project name or path
    pub project: String,
    /// Run build in background
    #[serde(default)]
    pub background: bool,
    /// Override default projects directory
    #[serde(default)]
    pub projects_dir: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BuildResult {
    pub success: bool,
    /// Build ID for background builds
    pub build_id: Option<String>,
    /// Build output (if not background)
    pub output: String,
    /// Path to built firmware binary
    pub artifact_path: Option<String>,
    /// Build duration in milliseconds
    pub duration_ms: Option<u64>,
}

// ============================================================================
// flash
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FlashArgs {
    /// Project name or path
    pub project: String,
    /// Serial port (e.g., "/dev/cu.usbserial-1110")
    #[serde(default)]
    pub port: Option<String>,
    /// Baud rate for flashing
    #[serde(default)]
    pub baud: Option<u32>,
    /// Override default projects directory
    #[serde(default)]
    pub projects_dir: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FlashResult {
    pub success: bool,
    pub output: String,
}

// ============================================================================
// clean
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CleanArgs {
    /// Project name or path
    pub project: String,
    /// Override default projects directory
    #[serde(default)]
    pub projects_dir: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CleanResult {
    pub success: bool,
    pub message: String,
}

// ============================================================================
// build_status
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BuildStatusArgs {
    /// Build ID from background build
    pub build_id: String,
}

#[derive(Debug, Serialize)]
pub struct BuildStatusResult {
    /// Status: "running", "complete", "failed"
    pub status: String,
    /// Current progress info if running
    pub progress: Option<String>,
    /// Build output if complete
    pub output: Option<String>,
    /// Path to artifact if complete
    pub artifact_path: Option<String>,
    /// Error message if failed
    pub error: Option<String>,
}

// ============================================================================
// monitor
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MonitorArgs {
    /// Project name or path
    pub project: String,
    /// Serial port (e.g., "/dev/cu.usbserial-1110")
    #[serde(default)]
    pub port: Option<String>,
    /// Capture duration in seconds (default 10)
    #[serde(default = "default_monitor_duration")]
    pub duration_seconds: u64,
    /// Override default projects directory
    #[serde(default)]
    pub projects_dir: Option<String>,
}

fn default_monitor_duration() -> u64 {
    10
}

#[derive(Debug, Serialize)]
pub struct MonitorResult {
    pub success: bool,
    /// Captured serial output
    pub output: String,
    /// Duration captured in seconds
    pub duration_seconds: u64,
}

// ============================================================================
// detect_device
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DetectDeviceArgs {}

#[derive(Debug, Serialize, Clone)]
pub struct DetectedDevice {
    /// Serial port path (e.g., "/dev/cu.usbserial-1110")
    pub port: String,
    /// USB Vendor ID : Product ID (e.g., "10c4:ea60")
    pub vid_pid: Option<String>,
    /// USB-UART bridge chip name (e.g., "CP2102", "CH340")
    pub bridge_chip: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DetectDeviceResult {
    /// Detected devices
    pub devices: Vec<DetectedDevice>,
}
