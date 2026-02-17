//! RMCP 0.3.2 implementation for Linux build MCP tools
//!
//! Provides 9 tools for Docker-based cross-compilation and SSH deployment.

use rmcp::{
    tool, tool_router, tool_handler, ServerHandler,
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::*,
    ErrorData as McpError,
    service::RequestContext,
    RoleServer,
};
use tracing::info;
use std::future::Future;
use std::path::Path;

use super::types::*;
use crate::config::Config;
use crate::docker_client;

/// Linux build tool handler
#[derive(Clone)]
pub struct LinuxBuildToolHandler {
    #[allow(dead_code)]
    tool_router: ToolRouter<LinuxBuildToolHandler>,
    config: Config,
}

impl LinuxBuildToolHandler {
    pub fn new(config: Config) -> Self {
        Self {
            tool_router: Self::tool_router(),
            config,
        }
    }
}

fn make_error(msg: impl Into<String>) -> McpError {
    McpError::internal_error(msg.into(), None)
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

        let container_id = docker_client::start_container(
            image,
            &name,
            workspace.as_deref(),
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
    // Deployment (3 tools)
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
}

#[tool_handler]
impl ServerHandler for LinuxBuildToolHandler {}
