//! RMCP 0.3.2 implementation for Linux build MCP tools
//!
//! Provides 17 tools for Docker-based cross-compilation, ADB/SSH deployment,
//! Yocto builds, and board connection management.

use rmcp::{
    tool, tool_router, tool_handler, ServerHandler,
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::*,
    ErrorData as McpError,
    service::RequestContext,
    RoleServer,
};
use tracing::info;
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use super::types::*;
use crate::adb_client;
use crate::config::Config;
use crate::docker_client;

// =========================================================================
// Yocto build state
// =========================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum YoctoBuildStatus {
    Running,
    Complete,
    Failed,
}

impl std::fmt::Display for YoctoBuildStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::Complete => write!(f, "complete"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct YoctoBuildState {
    pub status: YoctoBuildStatus,
    pub output: String,
    pub started_at: Instant,
    pub container: String,
    pub image: String,
}

// =========================================================================
// Board connection state
// =========================================================================

#[derive(Debug, Clone)]
pub enum BoardTransport {
    Ssh { ip: String, user: String, key: Option<PathBuf> },
    Adb { serial: Option<String> },
}

#[derive(Debug, Clone)]
pub struct BoardConnection {
    pub id: String,
    pub transport: BoardTransport,
    pub connected_at: Instant,
}

// =========================================================================
// Tool handler
// =========================================================================

/// Linux build tool handler
#[derive(Clone)]
pub struct LinuxBuildToolHandler {
    #[allow(dead_code)]
    tool_router: ToolRouter<LinuxBuildToolHandler>,
    config: Config,
    yocto_builds: Arc<RwLock<HashMap<String, YoctoBuildState>>>,
    boards: Arc<RwLock<HashMap<String, BoardConnection>>>,
}

impl LinuxBuildToolHandler {
    pub fn new(config: Config) -> Self {
        Self {
            tool_router: Self::tool_router(),
            config,
            yocto_builds: Arc::new(RwLock::new(HashMap::new())),
            boards: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for LinuxBuildToolHandler {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

fn make_error(msg: impl Into<String>) -> McpError {
    McpError::internal_error(msg.into(), None)
}

/// Truncate output to first + last lines for large Yocto builds
fn truncate_output(output: &str, head: usize, tail: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= head + tail {
        return output.to_string();
    }
    let mut result = lines[..head].join("\n");
    result.push_str(&format!(
        "\n\n... ({} lines omitted) ...\n\n",
        lines.len() - head - tail
    ));
    result.push_str(&lines[lines.len() - tail..].join("\n"));
    result
}

#[tool_router]
impl LinuxBuildToolHandler {
    // =========================================================================
    // Container Lifecycle (3 tools)
    // =========================================================================

    #[tool(description = "Start a Docker build container with optional workspace mount. Returns container name for use with other tools.")]
    async fn start_container(&self, Parameters(args): Parameters<StartContainerArgs>) -> Result<CallToolResult, McpError> {
        let name = args.name.unwrap_or_else(|| {
            format!("linux-build-{}", &uuid::Uuid::new_v4().to_string()[..8])
        });
        let image = args.image.as_deref().unwrap_or(&self.config.docker_image);
        let workspace = args.workspace_dir
            .as_ref()
            .map(|s| std::path::PathBuf::from(s))
            .or_else(|| self.config.workspace_dir.clone());

        let extra_volumes = args.extra_volumes.unwrap_or_default();

        let container_id = docker_client::start_container(
            image,
            &name,
            workspace.as_deref(),
            &extra_volumes,
        ).await.map_err(|e| make_error(e.to_string()))?;

        let message = format!(
            "Container started\n\
             Name: {}\n\
             Image: {}\n\
             Container ID: {}\n\
             Workspace: {}",
            name,
            image,
            &container_id[..12.min(container_id.len())],
            workspace.map(|p| p.display().to_string()).unwrap_or_else(|| "none".to_string()),
        );

        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    #[tool(description = "Stop and remove a Docker build container")]
    async fn stop_container(&self, Parameters(args): Parameters<StopContainerArgs>) -> Result<CallToolResult, McpError> {
        docker_client::stop_container(&args.container)
            .await.map_err(|e| make_error(e.to_string()))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Container '{}' stopped and removed", args.container
        ))]))
    }

    #[tool(description = "Check if a Docker build container is running")]
    async fn container_status(&self, Parameters(args): Parameters<ContainerStatusArgs>) -> Result<CallToolResult, McpError> {
        let state = docker_client::container_status(&args.container)
            .await.map_err(|e| make_error(e.to_string()))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Container '{}': {}", args.container, state
        ))]))
    }

    // =========================================================================
    // Build Operations (3 tools)
    // =========================================================================

    #[tool(description = "Execute a command inside the Docker build container")]
    async fn run_command(&self, Parameters(args): Parameters<RunCommandArgs>) -> Result<CallToolResult, McpError> {
        let result = docker_client::exec_command(
            &args.container,
            &args.command,
            args.workdir.as_deref(),
        ).await.map_err(|e| make_error(e.to_string()))?;

        let mut message = format!("Command: {}\nExit code: {}\n", args.command, result.exit_code);

        if !result.stdout.is_empty() {
            message.push_str(&format!("\nStdout:\n{}", result.stdout));
        }
        if !result.stderr.is_empty() {
            message.push_str(&format!("\nStderr:\n{}", result.stderr));
        }

        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    #[tool(description = "Run a build command in the Docker container (convenience wrapper around run_command)")]
    async fn build(&self, Parameters(args): Parameters<BuildArgs>) -> Result<CallToolResult, McpError> {
        info!("Building in container '{}': {}", args.container, args.command);

        let result = docker_client::exec_command(
            &args.container,
            &args.command,
            Some(&args.workdir),
        ).await.map_err(|e| make_error(e.to_string()))?;

        let status = if result.success { "SUCCESS" } else { "FAILED" };

        let mut message = format!("Build {}\nCommand: {}\nExit code: {}\n", status, args.command, result.exit_code);

        if !result.stdout.is_empty() {
            message.push_str(&format!("\nOutput:\n{}", result.stdout));
        }
        if !result.stderr.is_empty() {
            message.push_str(&format!("\nErrors:\n{}", result.stderr));
        }

        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    #[tool(description = "List available build artifacts in the container")]
    async fn list_artifacts(&self, Parameters(args): Parameters<ListArtifactsArgs>) -> Result<CallToolResult, McpError> {
        let result = docker_client::exec_command(
            &args.container,
            &format!("find {} -type f | head -100", args.container_path),
            None,
        ).await.map_err(|e| make_error(e.to_string()))?;

        if result.stdout.trim().is_empty() {
            Ok(CallToolResult::success(vec![Content::text(format!(
                "No artifacts found in {}", args.container_path
            ))]))
        } else {
            Ok(CallToolResult::success(vec![Content::text(format!(
                "Artifacts in {}:\n{}", args.container_path, result.stdout
            ))]))
        }
    }

    // =========================================================================
    // SSH Deployment (3 tools)
    // =========================================================================

    #[tool(description = "Copy build artifacts from container to host filesystem")]
    async fn collect_artifacts(&self, Parameters(args): Parameters<CollectArtifactsArgs>) -> Result<CallToolResult, McpError> {
        docker_client::copy_from_container(
            &args.container,
            &args.container_path,
            &args.host_path,
        ).await.map_err(|e| make_error(e.to_string()))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Copied {} from container '{}' to {}",
            args.container_path, args.container, args.host_path
        ))]))
    }

    #[tool(description = "Deploy a file to the board via SCP over SSH")]
    async fn deploy(&self, Parameters(args): Parameters<DeployArgs>) -> Result<CallToolResult, McpError> {
        let board_ip = args.board_ip
            .or_else(|| self.config.default_board_ip.clone())
            .ok_or_else(|| McpError::invalid_params(
                "No board IP specified. Pass board_ip parameter or use --board-ip CLI flag.".to_string(),
                None,
            ))?;

        if !Path::new(&args.file_path).exists() {
            return Err(McpError::invalid_params(
                format!("File not found: {}", args.file_path),
                None,
            ));
        }

        docker_client::scp_deploy(
            &args.file_path,
            &self.config.ssh_user,
            &board_ip,
            &args.remote_path,
            self.config.ssh_key.as_deref(),
        ).await.map_err(|e| make_error(e.to_string()))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Deployed {} to {}@{}:{}",
            args.file_path, self.config.ssh_user, board_ip, args.remote_path
        ))]))
    }

    #[tool(description = "Run a command on the board via SSH")]
    async fn ssh_command(&self, Parameters(args): Parameters<SshCommandArgs>) -> Result<CallToolResult, McpError> {
        let board_ip = args.board_ip
            .or_else(|| self.config.default_board_ip.clone())
            .ok_or_else(|| McpError::invalid_params(
                "No board IP specified. Pass board_ip parameter or use --board-ip CLI flag.".to_string(),
                None,
            ))?;

        let result = docker_client::ssh_command(
            &self.config.ssh_user,
            &board_ip,
            &args.command,
            self.config.ssh_key.as_deref(),
        ).await.map_err(|e| make_error(e.to_string()))?;

        let mut message = format!(
            "SSH {}@{}: {}\nExit code: {}\n",
            self.config.ssh_user, board_ip, args.command, result.exit_code
        );

        if !result.stdout.is_empty() {
            message.push_str(&format!("\nOutput:\n{}", result.stdout));
        }
        if !result.stderr.is_empty() {
            message.push_str(&format!("\nErrors:\n{}", result.stderr));
        }

        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    // =========================================================================
    // ADB Transport (3 tools)
    // =========================================================================

    #[tool(description = "Run a shell command on the board via ADB")]
    async fn adb_shell(&self, Parameters(args): Parameters<AdbShellArgs>) -> Result<CallToolResult, McpError> {
        if args.command.is_empty() {
            return Err(McpError::invalid_params(
                "Command cannot be empty".to_string(),
                None,
            ));
        }

        let serial = args.serial
            .or_else(|| self.config.default_adb_serial.clone());

        let result = adb_client::adb_shell(
            &args.command,
            serial.as_deref(),
        ).await.map_err(|e| make_error(e.to_string()))?;

        let mut message = format!(
            "ADB shell: {}\nExit code: {}\n",
            args.command, result.exit_code
        );

        if !result.stdout.is_empty() {
            message.push_str(&format!("\nOutput:\n{}", result.stdout));
        }
        if !result.stderr.is_empty() {
            message.push_str(&format!("\nErrors:\n{}", result.stderr));
        }

        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    #[tool(description = "Push a file to the board via ADB")]
    async fn adb_deploy(&self, Parameters(args): Parameters<AdbDeployArgs>) -> Result<CallToolResult, McpError> {
        if !Path::new(&args.file_path).exists() {
            return Err(McpError::invalid_params(
                format!("File not found: {}", args.file_path),
                None,
            ));
        }

        let serial = args.serial
            .or_else(|| self.config.default_adb_serial.clone());

        let output = adb_client::adb_push(
            &args.file_path,
            &args.remote_path,
            serial.as_deref(),
        ).await.map_err(|e| make_error(e.to_string()))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "ADB push {} -> {}\n{}", args.file_path, args.remote_path, output
        ))]))
    }

    #[tool(description = "Pull a file from the board via ADB")]
    async fn adb_pull(&self, Parameters(args): Parameters<AdbPullArgs>) -> Result<CallToolResult, McpError> {
        let serial = args.serial
            .or_else(|| self.config.default_adb_serial.clone());

        let output = adb_client::adb_pull(
            &args.remote_path,
            &args.local_path,
            serial.as_deref(),
        ).await.map_err(|e| make_error(e.to_string()))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "ADB pull {} -> {}\n{}", args.remote_path, args.local_path, output
        ))]))
    }

    // =========================================================================
    // Flash Image (1 tool)
    // =========================================================================

    #[tool(description = "Flash a compressed WIC image to a board's block device via SSH or ADB. Streams bzcat output through the selected transport.")]
    async fn flash_image(&self, Parameters(args): Parameters<FlashImageArgs>) -> Result<CallToolResult, McpError> {
        if !Path::new(&args.image_path).exists() {
            return Err(McpError::invalid_params(
                format!("Image file not found: {}", args.image_path),
                None,
            ));
        }

        match args.transport.as_str() {
            "ssh" => {
                let board_ip = args.board_ip
                    .or_else(|| self.config.default_board_ip.clone())
                    .ok_or_else(|| McpError::invalid_params(
                        "SSH transport requires board_ip".to_string(),
                        None,
                    ))?;

                let result = docker_client::flash_image_ssh(
                    &args.image_path,
                    &self.config.ssh_user,
                    &board_ip,
                    &args.device,
                    self.config.ssh_key.as_deref(),
                ).await.map_err(|e| make_error(e.to_string()))?;

                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Flash complete via SSH to {}@{}:{}\n{}",
                    self.config.ssh_user, board_ip, args.device, result
                ))]))
            }
            "adb" => {
                let serial = args.serial
                    .or_else(|| self.config.default_adb_serial.clone());

                let result = adb_client::flash_image_adb(
                    &args.image_path,
                    &args.device,
                    serial.as_deref(),
                ).await.map_err(|e| make_error(e.to_string()))?;

                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Flash complete via ADB to {}\n{}", args.device, result
                ))]))
            }
            other => Err(McpError::invalid_params(
                format!("Unknown transport '{}'. Use 'ssh' or 'adb'.", other),
                None,
            )),
        }
    }

    // =========================================================================
    // Yocto Build (2 tools)
    // =========================================================================

    #[tool(description = "Run a Yocto bitbake build in a Docker container. Supports background mode for long builds.")]
    async fn yocto_build(&self, Parameters(args): Parameters<YoctoBuildArgs>) -> Result<CallToolResult, McpError> {
        // Build the bitbake command
        let mut shell_cmd = format!(
            "cd /home/builder/yocto && source poky/oe-init-build-env {} > /dev/null",
            args.build_dir
        );

        if let Some(recipes) = &args.recipes_to_clean {
            if !recipes.is_empty() {
                shell_cmd.push_str(&format!(
                    " && bitbake -c cleansstate {}",
                    recipes.join(" ")
                ));
            }
        }

        shell_cmd.push_str(&format!(" && bitbake {}", args.image));

        if args.background {
            let build_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
            let state = YoctoBuildState {
                status: YoctoBuildStatus::Running,
                output: String::new(),
                started_at: Instant::now(),
                container: args.container.clone(),
                image: args.image.clone(),
            };

            self.yocto_builds.write().await.insert(build_id.clone(), state);

            let builds = self.yocto_builds.clone();
            let build_id_clone = build_id.clone();
            let container = args.container.clone();
            let shell_cmd_clone = shell_cmd.clone();

            tokio::spawn(async move {
                let result = docker_client::exec_command(
                    &container,
                    &shell_cmd_clone,
                    None,
                ).await;

                let mut builds = builds.write().await;
                if let Some(state) = builds.get_mut(&build_id_clone) {
                    match result {
                        Ok(exec_result) => {
                            let combined = format!("{}{}", exec_result.stdout, exec_result.stderr);
                            state.output = truncate_output(&combined, 20, 80);
                            state.status = if exec_result.success {
                                YoctoBuildStatus::Complete
                            } else {
                                YoctoBuildStatus::Failed
                            };
                        }
                        Err(e) => {
                            state.output = e.to_string();
                            state.status = YoctoBuildStatus::Failed;
                        }
                    }
                }
            });

            Ok(CallToolResult::success(vec![Content::text(format!(
                "Yocto build started in background\n\
                 Build ID: {}\n\
                 Container: {}\n\
                 Image: {}\n\
                 Command: {}\n\n\
                 Use yocto_build_status(build_id=\"{}\") to check progress.",
                build_id, args.container, args.image, shell_cmd, build_id
            ))]))
        } else {
            // Synchronous build
            let result = docker_client::exec_command(
                &args.container,
                &shell_cmd,
                None,
            ).await.map_err(|e| make_error(e.to_string()))?;

            let status = if result.success { "SUCCESS" } else { "FAILED" };
            let combined = format!("{}{}", result.stdout, result.stderr);
            let output = truncate_output(&combined, 20, 80);

            Ok(CallToolResult::success(vec![Content::text(format!(
                "Yocto build {}\nImage: {}\nExit code: {}\n\n{}",
                status, args.image, result.exit_code, output
            ))]))
        }
    }

    #[tool(description = "Check the status of a background Yocto build")]
    async fn yocto_build_status(&self, Parameters(args): Parameters<YoctoBuildStatusArgs>) -> Result<CallToolResult, McpError> {
        let builds = self.yocto_builds.read().await;
        let state = builds.get(&args.build_id).ok_or_else(|| {
            McpError::invalid_params(
                format!("Build ID '{}' not found", args.build_id),
                None,
            )
        })?;

        let elapsed = state.started_at.elapsed();
        let elapsed_str = format!("{}m {}s", elapsed.as_secs() / 60, elapsed.as_secs() % 60);

        let mut message = format!(
            "Build ID: {}\n\
             Status: {}\n\
             Container: {}\n\
             Image: {}\n\
             Elapsed: {}",
            args.build_id, state.status, state.container, state.image, elapsed_str
        );

        if !state.output.is_empty() {
            message.push_str(&format!("\n\nOutput:\n{}", state.output));
        }

        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    // =========================================================================
    // Board Connection (3 tools)
    // =========================================================================

    #[tool(description = "Register a board connection for use with other tools. Returns board_id. Transport: 'ssh', 'adb', or 'auto' (tries ADB first, falls back to SSH).")]
    async fn board_connect(&self, Parameters(args): Parameters<BoardConnectArgs>) -> Result<CallToolResult, McpError> {
        let board_id = uuid::Uuid::new_v4().to_string()[..8].to_string();

        let transport = match args.transport.as_str() {
            "ssh" => {
                let ip = args.board_ip
                    .or_else(|| self.config.default_board_ip.clone())
                    .ok_or_else(|| McpError::invalid_params(
                        "SSH transport requires board_ip".to_string(),
                        None,
                    ))?;
                let user = args.ssh_user.unwrap_or_else(|| self.config.ssh_user.clone());
                let key = args.ssh_key.map(PathBuf::from).or_else(|| self.config.ssh_key.clone());
                BoardTransport::Ssh { ip, user, key }
            }
            "adb" => {
                let serial = args.serial
                    .or_else(|| self.config.default_adb_serial.clone());
                BoardTransport::Adb { serial }
            }
            "auto" => {
                // Try ADB first
                match adb_client::adb_devices().await {
                    Ok(devices) if !devices.is_empty() => {
                        let serial = devices[0].serial.clone();
                        info!("Auto-detected ADB device: {}", serial);
                        BoardTransport::Adb { serial: Some(serial) }
                    }
                    _ => {
                        // Fall back to SSH
                        let ip = args.board_ip
                            .or_else(|| self.config.default_board_ip.clone())
                            .ok_or_else(|| McpError::invalid_params(
                                "Auto transport: no ADB devices found and no board_ip for SSH fallback".to_string(),
                                None,
                            ))?;
                        let user = args.ssh_user.unwrap_or_else(|| self.config.ssh_user.clone());
                        let key = args.ssh_key.map(PathBuf::from).or_else(|| self.config.ssh_key.clone());
                        info!("Auto: no ADB devices, falling back to SSH {}@{}", user, ip);
                        BoardTransport::Ssh { ip, user, key }
                    }
                }
            }
            other => return Err(McpError::invalid_params(
                format!("Unknown transport '{}'. Use 'ssh', 'adb', or 'auto'.", other),
                None,
            )),
        };

        let transport_desc = match &transport {
            BoardTransport::Ssh { ip, user, .. } => format!("SSH {}@{}", user, ip),
            BoardTransport::Adb { serial } => format!(
                "ADB{}", serial.as_ref().map(|s| format!(" ({})", s)).unwrap_or_default()
            ),
        };

        let conn = BoardConnection {
            id: board_id.clone(),
            transport,
            connected_at: Instant::now(),
        };

        self.boards.write().await.insert(board_id.clone(), conn);

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Board connected\n\
             Board ID: {}\n\
             Transport: {}",
            board_id, transport_desc
        ))]))
    }

    #[tool(description = "Remove a board connection")]
    async fn board_disconnect(&self, Parameters(args): Parameters<BoardDisconnectArgs>) -> Result<CallToolResult, McpError> {
        let removed = self.boards.write().await.remove(&args.board_id);

        match removed {
            Some(_) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Board '{}' disconnected", args.board_id
            ))])),
            None => Err(McpError::invalid_params(
                format!("Board ID '{}' not found", args.board_id),
                None,
            )),
        }
    }

    #[tool(description = "Check board connection status. If board_id is omitted, lists all connections.")]
    async fn board_status(&self, Parameters(args): Parameters<BoardStatusArgs>) -> Result<CallToolResult, McpError> {
        let boards = self.boards.read().await;

        if let Some(board_id) = &args.board_id {
            let conn = boards.get(board_id).ok_or_else(|| {
                McpError::invalid_params(
                    format!("Board ID '{}' not found", board_id),
                    None,
                )
            })?;

            let elapsed = conn.connected_at.elapsed();
            let transport_desc = match &conn.transport {
                BoardTransport::Ssh { ip, user, .. } => format!("SSH {}@{}", user, ip),
                BoardTransport::Adb { serial } => format!(
                    "ADB{}", serial.as_ref().map(|s| format!(" ({})", s)).unwrap_or_default()
                ),
            };

            Ok(CallToolResult::success(vec![Content::text(format!(
                "Board ID: {}\n\
                 Transport: {}\n\
                 Connected: {}m {}s ago",
                board_id, transport_desc,
                elapsed.as_secs() / 60, elapsed.as_secs() % 60
            ))]))
        } else {
            if boards.is_empty() {
                return Ok(CallToolResult::success(vec![Content::text(
                    "No board connections".to_string()
                )]));
            }

            let mut lines = vec!["Board connections:".to_string()];
            for (id, conn) in boards.iter() {
                let transport_desc = match &conn.transport {
                    BoardTransport::Ssh { ip, user, .. } => format!("SSH {}@{}", user, ip),
                    BoardTransport::Adb { serial } => format!(
                        "ADB{}", serial.as_ref().map(|s| format!(" ({})", s)).unwrap_or_default()
                    ),
                };
                let elapsed = conn.connected_at.elapsed();
                lines.push(format!(
                    "  {} — {} ({}m ago)",
                    id, transport_desc, elapsed.as_secs() / 60
                ));
            }

            Ok(CallToolResult::success(vec![Content::text(lines.join("\n"))]))
        }
    }
}

const TOOL_COUNT: usize = 17;

#[tool_handler]
impl ServerHandler for LinuxBuildToolHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(format!(
                "Linux Build MCP Server - Docker-based cross-compilation and SSH/ADB deployment. \
                 {} tools available: start_container, stop_container, container_status, \
                 run_command, build, list_artifacts, collect_artifacts, deploy, ssh_command, \
                 adb_shell, adb_deploy, adb_pull, flash_image, yocto_build, yocto_build_status, \
                 board_connect, board_disconnect, board_status.",
                TOOL_COUNT,
            )),
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        info!("Linux Build MCP server initialized with {} tools", TOOL_COUNT);
        Ok(self.get_info())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::handler::server::tool::Parameters;

    /// Extract text from a CallToolResult's first content element
    fn extract_text(result: &CallToolResult) -> &str {
        result.content[0].as_text().expect("expected text content").text.as_str()
    }

    #[test]
    fn test_handler_construction() {
        let handler = LinuxBuildToolHandler::default();
        assert_eq!(handler.config.docker_image, "stm32mp1-sdk");
        assert_eq!(handler.config.ssh_user, "root");
        assert!(handler.config.default_board_ip.is_none());
        assert!(handler.config.default_adb_serial.is_none());
    }

    #[test]
    fn test_handler_with_custom_config() {
        let config = Config {
            docker_image: "custom-sdk:v2".to_string(),
            workspace_dir: Some("/tmp/ws".into()),
            default_board_ip: Some("192.168.1.50".to_string()),
            ssh_key: Some("/home/user/.ssh/id_ed25519".into()),
            ssh_user: "admin".to_string(),
            default_adb_serial: Some("ABC123".to_string()),
        };
        let handler = LinuxBuildToolHandler::new(config.clone());
        assert_eq!(handler.config.docker_image, "custom-sdk:v2");
        assert_eq!(handler.config.ssh_user, "admin");
        assert_eq!(handler.config.default_board_ip.unwrap(), "192.168.1.50");
        assert_eq!(handler.config.default_adb_serial.unwrap(), "ABC123");
    }

    #[test]
    fn test_server_info() {
        let handler = LinuxBuildToolHandler::default();
        let info = handler.get_info();
        let instructions = info.instructions.unwrap();
        assert!(instructions.contains("17 tools"));
        assert!(instructions.contains("adb_shell"));
        assert!(instructions.contains("flash_image"));
        assert!(instructions.contains("yocto_build"));
        assert!(instructions.contains("board_connect"));
    }

    #[tokio::test]
    async fn test_deploy_missing_file() {
        let config = Config {
            default_board_ip: Some("10.0.0.1".to_string()),
            ..Config::default()
        };
        let handler = LinuxBuildToolHandler::new(config);
        let result = handler
            .deploy(Parameters(DeployArgs {
                file_path: "/nonexistent/file.bin".to_string(),
                remote_path: "/home/root/".to_string(),
                board_ip: None,
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("File not found"));
    }

    #[tokio::test]
    async fn test_deploy_no_board_ip() {
        let handler = LinuxBuildToolHandler::default();
        let result = handler
            .deploy(Parameters(DeployArgs {
                file_path: "/tmp/test.bin".to_string(),
                remote_path: "/home/root/".to_string(),
                board_ip: None,
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("No board IP"));
    }

    #[tokio::test]
    async fn test_ssh_command_no_board_ip() {
        let handler = LinuxBuildToolHandler::default();
        let result = handler
            .ssh_command(Parameters(SshCommandArgs {
                command: "uname -a".to_string(),
                board_ip: None,
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("No board IP"));
    }

    #[tokio::test]
    async fn test_container_status_not_found() {
        let handler = LinuxBuildToolHandler::default();
        let result = handler
            .container_status(Parameters(ContainerStatusArgs {
                container: "nonexistent-container-xyz".to_string(),
            }))
            .await;

        // Docker inspect returns "not found" for nonexistent containers
        // This should succeed with a "not found" status (not error)
        match result {
            Ok(r) => {
                let text = extract_text(&r);
                assert!(text.contains("not found") || text.contains("nonexistent"));
            }
            Err(_) => {
                // Also acceptable if docker is not installed
            }
        }
    }

    #[test]
    fn test_make_error() {
        let err = make_error("test error message");
        assert!(err.to_string().contains("test error message"));
    }

    // ADB tool tests

    #[tokio::test]
    async fn test_adb_shell_empty_command() {
        let handler = LinuxBuildToolHandler::default();
        let result = handler
            .adb_shell(Parameters(AdbShellArgs {
                command: "".to_string(),
                serial: None,
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Command cannot be empty"));
    }

    #[tokio::test]
    async fn test_adb_deploy_missing_file() {
        let handler = LinuxBuildToolHandler::default();
        let result = handler
            .adb_deploy(Parameters(AdbDeployArgs {
                file_path: "/nonexistent/file.bin".to_string(),
                remote_path: "/data/local/tmp/".to_string(),
                serial: None,
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("File not found"));
    }

    // Flash image tests

    #[tokio::test]
    async fn test_flash_image_missing_file() {
        let handler = LinuxBuildToolHandler::default();
        let result = handler
            .flash_image(Parameters(FlashImageArgs {
                image_path: "/nonexistent/image.wic.bz2".to_string(),
                transport: "ssh".to_string(),
                device: "/dev/mmcblk1".to_string(),
                board_ip: Some("10.0.0.1".to_string()),
                serial: None,
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Image file not found"));
    }

    #[tokio::test]
    async fn test_flash_image_unknown_transport() {
        let handler = LinuxBuildToolHandler::default();
        // Create a temp file so the file-exists check passes
        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        let result = handler
            .flash_image(Parameters(FlashImageArgs {
                image_path: tmpfile.path().to_string_lossy().to_string(),
                transport: "uart".to_string(),
                device: "/dev/mmcblk1".to_string(),
                board_ip: None,
                serial: None,
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unknown transport"));
    }

    #[tokio::test]
    async fn test_flash_image_ssh_no_board_ip() {
        let handler = LinuxBuildToolHandler::default();
        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        let result = handler
            .flash_image(Parameters(FlashImageArgs {
                image_path: tmpfile.path().to_string_lossy().to_string(),
                transport: "ssh".to_string(),
                device: "/dev/mmcblk1".to_string(),
                board_ip: None,
                serial: None,
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("requires board_ip"));
    }

    // Yocto build status tests

    #[tokio::test]
    async fn test_yocto_build_status_not_found() {
        let handler = LinuxBuildToolHandler::default();
        let result = handler
            .yocto_build_status(Parameters(YoctoBuildStatusArgs {
                build_id: "nonexistent".to_string(),
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    // Board connection tests

    #[tokio::test]
    async fn test_board_connect_ssh() {
        let handler = LinuxBuildToolHandler::default();
        let result = handler
            .board_connect(Parameters(BoardConnectArgs {
                transport: "ssh".to_string(),
                board_ip: Some("10.0.0.1".to_string()),
                serial: None,
                ssh_key: None,
                ssh_user: None,
            }))
            .await;

        assert!(result.is_ok());
        let result = result.unwrap();
        let text = extract_text(&result);
        assert!(text.contains("Board connected"));
        assert!(text.contains("SSH root@10.0.0.1"));
    }

    #[tokio::test]
    async fn test_board_connect_adb() {
        let handler = LinuxBuildToolHandler::default();
        let result = handler
            .board_connect(Parameters(BoardConnectArgs {
                transport: "adb".to_string(),
                board_ip: None,
                serial: Some("ABC123".to_string()),
                ssh_key: None,
                ssh_user: None,
            }))
            .await;

        assert!(result.is_ok());
        let result = result.unwrap();
        let text = extract_text(&result);
        assert!(text.contains("Board connected"));
        assert!(text.contains("ADB (ABC123)"));
    }

    #[tokio::test]
    async fn test_board_connect_ssh_no_ip() {
        let handler = LinuxBuildToolHandler::default();
        let result = handler
            .board_connect(Parameters(BoardConnectArgs {
                transport: "ssh".to_string(),
                board_ip: None,
                serial: None,
                ssh_key: None,
                ssh_user: None,
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("requires board_ip"));
    }

    #[tokio::test]
    async fn test_board_connect_unknown_transport() {
        let handler = LinuxBuildToolHandler::default();
        let result = handler
            .board_connect(Parameters(BoardConnectArgs {
                transport: "uart".to_string(),
                board_ip: None,
                serial: None,
                ssh_key: None,
                ssh_user: None,
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unknown transport"));
    }

    #[tokio::test]
    async fn test_board_disconnect() {
        let handler = LinuxBuildToolHandler::default();

        // Connect first
        let result = handler
            .board_connect(Parameters(BoardConnectArgs {
                transport: "adb".to_string(),
                board_ip: None,
                serial: None,
                ssh_key: None,
                ssh_user: None,
            }))
            .await
            .unwrap();
        let text = extract_text(&result);
        let board_id = text.lines()
            .find(|l| l.starts_with("Board ID:"))
            .unwrap()
            .split(": ")
            .nth(1)
            .unwrap()
            .to_string();

        // Disconnect
        let result = handler
            .board_disconnect(Parameters(BoardDisconnectArgs {
                board_id: board_id.clone(),
            }))
            .await;
        assert!(result.is_ok());

        // Disconnect again should fail
        let result = handler
            .board_disconnect(Parameters(BoardDisconnectArgs {
                board_id,
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_board_status_empty() {
        let handler = LinuxBuildToolHandler::default();
        let result = handler
            .board_status(Parameters(BoardStatusArgs {
                board_id: None,
            }))
            .await;

        assert!(result.is_ok());
        let result = result.unwrap();
        let text = extract_text(&result);
        assert!(text.contains("No board connections"));
    }

    #[tokio::test]
    async fn test_board_status_with_connection() {
        let handler = LinuxBuildToolHandler::default();

        // Connect
        let result = handler
            .board_connect(Parameters(BoardConnectArgs {
                transport: "ssh".to_string(),
                board_ip: Some("192.168.1.1".to_string()),
                serial: None,
                ssh_key: None,
                ssh_user: Some("pi".to_string()),
            }))
            .await
            .unwrap();
        let text = extract_text(&result);
        let board_id = text.lines()
            .find(|l| l.starts_with("Board ID:"))
            .unwrap()
            .split(": ")
            .nth(1)
            .unwrap()
            .to_string();

        // Check specific board
        let result = handler
            .board_status(Parameters(BoardStatusArgs {
                board_id: Some(board_id),
            }))
            .await;
        assert!(result.is_ok());
        let result = result.unwrap();
        let text = extract_text(&result);
        assert!(text.contains("SSH pi@192.168.1.1"));
    }

    // Truncation tests

    #[test]
    fn test_truncate_output_short() {
        let input = "line1\nline2\nline3";
        assert_eq!(truncate_output(input, 20, 80), input);
    }

    #[test]
    fn test_truncate_output_long() {
        let lines: Vec<String> = (0..200).map(|i| format!("line {}", i)).collect();
        let input = lines.join("\n");
        let result = truncate_output(&input, 5, 5);
        assert!(result.contains("line 0"));
        assert!(result.contains("line 4"));
        assert!(result.contains("190 lines omitted"));
        assert!(result.contains("line 195"));
        assert!(result.contains("line 199"));
    }
}
