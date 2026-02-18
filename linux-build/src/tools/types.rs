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
    /// Additional Docker volume mounts (e.g., "yocto-data:/home/builder/yocto")
    #[serde(default)]
    pub extra_volumes: Option<Vec<String>>,
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

// ============================================================================
// adb_shell
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AdbShellArgs {
    /// Shell command to execute on the device
    pub command: String,
    /// ADB device serial (uses default from config if omitted)
    #[serde(default)]
    pub serial: Option<String>,
}

// ============================================================================
// adb_deploy
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AdbDeployArgs {
    /// Local file path to push to device
    pub file_path: String,
    /// Remote path on the device
    #[serde(default = "default_adb_remote_path")]
    pub remote_path: String,
    /// ADB device serial (uses default from config if omitted)
    #[serde(default)]
    pub serial: Option<String>,
}

fn default_adb_remote_path() -> String { "/data/local/tmp/".to_string() }

// ============================================================================
// adb_pull
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AdbPullArgs {
    /// Remote file path on the device
    pub remote_path: String,
    /// Local destination path
    pub local_path: String,
    /// ADB device serial (uses default from config if omitted)
    #[serde(default)]
    pub serial: Option<String>,
}

// ============================================================================
// flash_image
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FlashImageArgs {
    /// Path to .wic.bz2 image file
    pub image_path: String,
    /// Transport method: "ssh" or "adb"
    pub transport: String,
    /// Target block device (default: /dev/mmcblk1)
    #[serde(default = "default_flash_device")]
    pub device: String,
    /// Board IP address (required for SSH transport)
    #[serde(default)]
    pub board_ip: Option<String>,
    /// ADB device serial (optional for ADB transport)
    #[serde(default)]
    pub serial: Option<String>,
}

fn default_flash_device() -> String { "/dev/mmcblk1".to_string() }

// ============================================================================
// yocto_build
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct YoctoBuildArgs {
    /// Container name or ID running the Yocto build environment
    pub container: String,
    /// Build directory name (default: build)
    #[serde(default = "default_yocto_build_dir")]
    pub build_dir: String,
    /// Bitbake image target (default: core-image-minimal)
    #[serde(default = "default_yocto_image")]
    pub image: String,
    /// Recipes to cleansstate before building
    #[serde(default)]
    pub recipes_to_clean: Option<Vec<String>>,
    /// Run build in background and return build_id immediately
    #[serde(default)]
    pub background: bool,
}

fn default_yocto_build_dir() -> String { "build".to_string() }
fn default_yocto_image() -> String { "core-image-minimal".to_string() }

// ============================================================================
// yocto_build_status
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct YoctoBuildStatusArgs {
    /// Build ID returned by yocto_build with background=true
    pub build_id: String,
}

// ============================================================================
// board_connect
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BoardConnectArgs {
    /// Transport type: "ssh", "adb", or "auto"
    pub transport: String,
    /// Board IP address (required for SSH transport)
    #[serde(default)]
    pub board_ip: Option<String>,
    /// ADB device serial
    #[serde(default)]
    pub serial: Option<String>,
    /// SSH key path
    #[serde(default)]
    pub ssh_key: Option<String>,
    /// SSH user (default: from config)
    #[serde(default)]
    pub ssh_user: Option<String>,
}

// ============================================================================
// board_disconnect
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BoardDisconnectArgs {
    /// Board connection ID
    pub board_id: String,
}

// ============================================================================
// board_status
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BoardStatusArgs {
    /// Board connection ID (if omitted, lists all connections)
    #[serde(default)]
    pub board_id: Option<String>,
}
