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
    /// Boards that have been built (from per-board build subdirectories)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub built_boards: Option<Vec<String>>,
    /// Description from manifest.yml
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Target boards from manifest.yml
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_boards: Option<Vec<String>>,
    /// Libraries from manifest.yml
    #[serde(skip_serializing_if = "Option::is_none")]
    pub libraries: Option<Vec<String>>,
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
    /// Board to clean (e.g., "nrf52840dk/nrf52840"). If omitted, cleans all board builds.
    #[serde(default)]
    pub board: Option<String>,
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

// ============================================================================
// run_tests
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunTestsArgs {
    /// Test path filter relative to apps parent dir (e.g., "lib/crash_log")
    #[serde(default)]
    pub path: Option<String>,
    /// Platform to test on (e.g., "qemu_cortex_m3")
    pub board: String,
    /// Test name filter (-k pattern)
    #[serde(default)]
    pub filter: Option<String>,
    /// Additional twister arguments
    #[serde(default)]
    pub extra_args: Option<String>,
    /// Run tests in background
    #[serde(default)]
    pub background: bool,
    /// Override workspace path
    #[serde(default)]
    pub workspace_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RunTestsResult {
    pub success: bool,
    /// Test run ID for background runs
    pub test_id: Option<String>,
    /// Parsed summary (foreground only)
    pub summary: Option<TestSummary>,
    /// Raw twister output
    pub output: String,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

// ============================================================================
// test_status
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TestStatusArgs {
    /// Test run ID from background run
    pub test_id: String,
}

#[derive(Debug, Serialize)]
pub struct TestStatusResult {
    /// Status: "running", "complete", "failed"
    pub status: String,
    /// Progress info if running
    pub progress: Option<String>,
    /// Parsed summary if complete
    pub summary: Option<TestSummary>,
    /// Raw output if complete
    pub output: Option<String>,
    /// Error message if failed
    pub error: Option<String>,
}

// ============================================================================
// test_results
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TestResultsArgs {
    /// Test run ID from background run
    #[serde(default)]
    pub test_id: Option<String>,
    /// Path to existing twister output directory
    #[serde(default)]
    pub results_dir: Option<String>,
    /// Override workspace path
    #[serde(default)]
    pub workspace_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TestResultsResult {
    pub summary: TestSummary,
    pub test_suites: Vec<TestSuiteResult>,
    pub failures: Vec<TestFailure>,
}

// ============================================================================
// shared test types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSummary {
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub errors: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestSuiteResult {
    pub name: String,
    pub platform: String,
    pub status: String,
    pub duration_ms: u64,
    pub used_ram: Option<u64>,
    pub used_rom: Option<u64>,
    pub test_cases: Vec<TestCaseResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestCaseResult {
    pub name: String,
    pub status: String,
    pub duration_ms: u64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestFailure {
    pub suite_name: String,
    pub test_name: Option<String>,
    pub platform: String,
    pub log: String,
}

// ============================================================================
// list_templates
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListTemplatesArgs {}

#[derive(Debug, Serialize)]
pub struct ListTemplatesResult {
    pub templates: Vec<TemplateInfo>,
}

#[derive(Debug, Serialize, Clone)]
pub struct TemplateInfo {
    /// Template name (e.g., "core")
    pub name: String,
    /// What this template provides
    pub description: String,
    /// Libraries included by default
    pub default_libraries: Vec<String>,
    /// Files that will be generated
    pub files: Vec<String>,
}

// ============================================================================
// create_app
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateAppArgs {
    /// Application name (lowercase alphanumeric + underscore)
    pub name: String,
    /// Template to use (defaults to "core")
    #[serde(default)]
    pub template: Option<String>,
    /// Default target board
    #[serde(default)]
    pub board: Option<String>,
    /// Additional libraries beyond template defaults
    #[serde(default)]
    pub libraries: Option<Vec<String>>,
    /// One-line description
    #[serde(default)]
    pub description: Option<String>,
    /// Override workspace path
    #[serde(default)]
    pub workspace_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateAppResult {
    pub success: bool,
    pub app_name: String,
    pub app_path: String,
    pub files_created: Vec<String>,
    pub message: String,
}

// ============================================================================
// manifests
// ============================================================================

/// Library manifest (lib/<name>/manifest.yml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryManifest {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub default_overlays: Vec<String>,
    #[serde(default)]
    pub board_overlays: bool,
    #[serde(default)]
    pub depends: Vec<String>,
}

/// App manifest (apps/<name>/manifest.yml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppManifest {
    pub description: String,
    #[serde(default)]
    pub boards: Vec<String>,
    #[serde(default)]
    pub libraries: Vec<String>,
    #[serde(default)]
    pub template: Option<String>,
}
