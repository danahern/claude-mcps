//! Type definitions for Linux build MCP tools

use serde::Deserialize;
use schemars::JsonSchema;

// ============================================================================
// start_container
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StartContainerArgs {
    /// Container name (default: auto-generated)
    #[serde(default)]
    pub name: Option<String>,
    /// Docker image override (default: from config)
    #[serde(default)]
    pub image: Option<String>,
    /// Host workspace directory to mount
    #[serde(default)]
    pub workspace_dir: Option<String>,
}

// ============================================================================
// stop_container
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StopContainerArgs {
    /// Container name or ID
    pub container: String,
}

// ============================================================================
// container_status
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContainerStatusArgs {
    /// Container name or ID
    pub container: String,
}

// ============================================================================
// run_command
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunCommandArgs {
    /// Container name or ID
    pub container: String,
    /// Shell command to execute
    pub command: String,
    /// Working directory inside container
    #[serde(default)]
    pub workdir: Option<String>,
}

// ============================================================================
// build
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BuildArgs {
    /// Container name or ID
    pub container: String,
    /// Build command to run (e.g., "make -j$(nproc)")
    #[serde(default = "default_build_cmd")]
    pub command: String,
    /// Working directory inside container (default: /workspace)
    #[serde(default = "default_workdir")]
    pub workdir: String,
}

fn default_build_cmd() -> String { "make".to_string() }
fn default_workdir() -> String { "/workspace".to_string() }

// ============================================================================
// collect_artifacts
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CollectArtifactsArgs {
    /// Container name or ID
    pub container: String,
    /// Path inside container to copy from (default: /artifacts)
    #[serde(default = "default_artifacts_path")]
    pub container_path: String,
    /// Host destination path
    pub host_path: String,
}

fn default_artifacts_path() -> String { "/artifacts".to_string() }

// ============================================================================
// deploy
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeployArgs {
    /// Local file path to deploy
    pub file_path: String,
    /// Remote path on the board
    #[serde(default = "default_remote_path")]
    pub remote_path: String,
    /// Board IP address (uses default from config if omitted)
    #[serde(default)]
    pub board_ip: Option<String>,
}

fn default_remote_path() -> String { "/home/root/".to_string() }

// ============================================================================
// ssh_command
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SshCommandArgs {
    /// Shell command to execute on the board
    pub command: String,
    /// Board IP address (uses default from config if omitted)
    #[serde(default)]
    pub board_ip: Option<String>,
}

// ============================================================================
// list_artifacts
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListArtifactsArgs {
    /// Container name or ID
    pub container: String,
    /// Path inside container to list (default: /artifacts)
    #[serde(default = "default_artifacts_path")]
    pub container_path: String,
}
