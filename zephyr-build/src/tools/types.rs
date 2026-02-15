//! Type definitions for Zephyr build MCP tools

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

// ============================================================================
// list_apps
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListAppsArgs {
    /// Override default workspace path
    #[serde(default)]
    pub workspace_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListAppsResult {
    pub apps: Vec<AppInfo>,
}

#[derive(Debug, Serialize, Clone)]
pub struct AppInfo {
    /// Application name (e.g., "ble_wifi_bridge")
    pub name: String,
    /// Path relative to workspace (e.g., "zephyr-apps/apps/ble_wifi_bridge")
    pub path: String,
    /// Whether a build directory exists
    pub has_build: bool,
    /// Board from last build (if available)
    pub board: Option<String>,
}

// ============================================================================
// list_boards
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListBoardsArgs {
    /// Filter pattern (e.g., "nrf" to find Nordic boards)
    #[serde(default)]
    pub filter: Option<String>,
    /// Include all boards from west (slow)
    #[serde(default)]
    pub include_all: bool,
}

#[derive(Debug, Serialize)]
pub struct ListBoardsResult {
    pub boards: Vec<BoardInfo>,
}

#[derive(Debug, Serialize, Clone)]
pub struct BoardInfo {
    /// Board identifier (e.g., "nrf52840dk/nrf52840")
    pub name: String,
    /// Architecture (e.g., "arm", "riscv", "xtensa")
    pub arch: String,
    /// Vendor name
    pub vendor: Option<String>,
}

// ============================================================================
// build
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BuildArgs {
    /// Application name or path
    pub app: String,
    /// Board identifier (e.g., "nrf52840dk/nrf52840")
    pub board: String,
    /// Use --pristine flag for clean build
    #[serde(default)]
    pub pristine: bool,
    /// Additional arguments for west/cmake
    #[serde(default)]
    pub extra_args: Option<String>,
    /// Run build in background
    #[serde(default)]
    pub background: bool,
    /// Override workspace path
    #[serde(default)]
    pub workspace_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BuildResult {
    pub success: bool,
    /// Build ID for background builds
    pub build_id: Option<String>,
    /// Build output (if not background)
    pub output: String,
    /// Path to built artifact (zephyr.elf)
    pub artifact_path: Option<String>,
    /// Build duration in milliseconds
    pub duration_ms: Option<u64>,
}

// ============================================================================
// clean
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CleanArgs {
    /// Application name or path
    pub app: String,
    /// Override workspace path
    #[serde(default)]
    pub workspace_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CleanResult {
    pub success: bool,
    pub message: String,
}

// ============================================================================
// build_all
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BuildAllArgs {
    /// Board identifier (e.g., "nrf52840dk/nrf52840")
    pub board: String,
    /// Use --pristine flag for clean builds
    #[serde(default)]
    pub pristine: bool,
    /// Override workspace path
    #[serde(default)]
    pub workspace_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BuildAllResult {
    /// Total number of apps found
    pub total: usize,
    /// Number of successful builds
    pub succeeded: usize,
    /// Number of failed builds
    pub failed: usize,
    /// Results for each app
    pub results: Vec<AppBuildResult>,
    /// Total duration in milliseconds
    pub duration_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct AppBuildResult {
    /// Application name
    pub app: String,
    /// Whether the build succeeded
    pub success: bool,
    /// Path to built artifact (if successful)
    pub artifact_path: Option<String>,
    /// Error output (if failed)
    pub error: Option<String>,
    /// Build duration in milliseconds
    pub duration_ms: u64,
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
    /// Current build phase if available
    pub progress: Option<String>,
    /// Build output if complete
    pub output: Option<String>,
    /// Path to artifact if complete
    pub artifact_path: Option<String>,
    /// Error message if failed
    pub error: Option<String>,
}
