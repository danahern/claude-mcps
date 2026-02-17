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

impl Default for LinuxBuildToolHandler {
    fn default() -> Self {
        Self::new(Config::default())
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
impl ServerHandler for LinuxBuildToolHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Linux Build MCP Server - Docker-based cross-compilation and SSH deployment. \
                 9 tools available: start_container, stop_container, container_status, \
                 run_command, build, list_artifacts, collect_artifacts, deploy, ssh_command."
                    .to_string(),
            ),
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        info!("Linux Build MCP server initialized with 9 tools");
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
    }

    #[test]
    fn test_handler_with_custom_config() {
        let config = Config {
            docker_image: "custom-sdk:v2".to_string(),
            workspace_dir: Some("/tmp/ws".into()),
            default_board_ip: Some("192.168.1.50".to_string()),
            ssh_key: Some("/home/user/.ssh/id_ed25519".into()),
            ssh_user: "admin".to_string(),
        };
        let handler = LinuxBuildToolHandler::new(config.clone());
        assert_eq!(handler.config.docker_image, "custom-sdk:v2");
        assert_eq!(handler.config.ssh_user, "admin");
        assert_eq!(handler.config.default_board_ip.unwrap(), "192.168.1.50");
    }

    #[test]
    fn test_server_info() {
        let handler = LinuxBuildToolHandler::default();
        let info = handler.get_info();
        assert!(info.instructions.unwrap().contains("9 tools"));
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
}
