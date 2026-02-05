//! Complete RMCP 0.3.2 implementation for Zephyr build MCP tools
//!
//! This implementation provides 5 build tools using west CLI subprocess calls

use rmcp::{
    tool, tool_router, tool_handler, ServerHandler,
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::*,
    ErrorData as McpError,
    service::RequestContext,
    RoleServer,
};
use tracing::{debug, error, info};
use std::future::Future;
use std::collections::HashMap;
use std::sync::Arc;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::sync::RwLock;
use tokio::process::Command;

use super::types::*;
use crate::config::Config;

/// Common boards for quick listing (without running west boards)
const COMMON_BOARDS: &[(&str, &str, &str)] = &[
    ("nrf52840dk/nrf52840", "arm", "Nordic"),
    ("nrf5340dk/nrf5340/cpuapp", "arm", "Nordic"),
    ("nrf54l15dk/nrf54l15/cpuapp", "arm", "Nordic"),
    ("esp32_devkitc/esp32/procpu", "xtensa", "Espressif"),
    ("esp32s3_eye/esp32s3/procpu", "xtensa", "Espressif"),
    ("esp32c3_devkitc", "riscv", "Espressif"),
    ("esp32c6_devkitc", "riscv", "Espressif"),
    ("stm32f4_disco", "arm", "ST"),
    ("nucleo_f411re", "arm", "ST"),
    ("nucleo_h743zi", "arm", "ST"),
    ("native_sim", "posix", "Zephyr"),
    ("qemu_cortex_m3", "arm", "QEMU"),
];

/// Build state for background builds
#[derive(Debug, Clone)]
pub struct BuildState {
    pub status: BuildStatus,
    pub output: String,
    pub started_at: Instant,
    pub app: String,
    pub board: String,
    pub artifact_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BuildStatus {
    Running,
    Complete,
    Failed,
}

/// Zephyr build tool handler with all 5 tools
#[derive(Clone)]
pub struct ZephyrBuildToolHandler {
    #[allow(dead_code)]
    tool_router: ToolRouter<ZephyrBuildToolHandler>,
    config: Config,
    builds: Arc<RwLock<HashMap<String, BuildState>>>,
}

impl ZephyrBuildToolHandler {
    pub fn new(config: Config) -> Self {
        Self {
            tool_router: Self::tool_router(),
            config,
            builds: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Find workspace path from config, env, or by looking for .west/
    fn find_workspace(&self, override_path: Option<&str>) -> Result<PathBuf, McpError> {
        // 1. Check override
        if let Some(path) = override_path {
            let p = PathBuf::from(path);
            if p.exists() {
                return Ok(p);
            }
            return Err(McpError::invalid_params(
                format!("Workspace path does not exist: {}", path),
                None,
            ));
        }

        // 2. Check config
        if let Some(path) = &self.config.workspace.path {
            if path.exists() {
                return Ok(path.clone());
            }
        }

        // 3. Check ZEPHYR_WORKSPACE env var
        if let Ok(path) = std::env::var("ZEPHYR_WORKSPACE") {
            let p = PathBuf::from(&path);
            if p.exists() {
                return Ok(p);
            }
        }

        // 4. Look for .west/ in current or parent directories
        let mut current = std::env::current_dir().map_err(|e| {
            McpError::internal_error(format!("Failed to get current directory: {}", e), None)
        })?;

        for _ in 0..10 {
            let west_dir = current.join(".west");
            if west_dir.exists() {
                return Ok(current);
            }
            if !current.pop() {
                break;
            }
        }

        Err(McpError::invalid_params(
            "Could not find Zephyr workspace. Please set --workspace, ZEPHYR_WORKSPACE env var, or run from a west workspace.".to_string(),
            None,
        ))
    }

    /// Get apps directory path
    fn get_apps_dir(&self, workspace: &Path) -> PathBuf {
        workspace.join(&self.config.workspace.apps_dir)
    }

    /// Find app path (handles both name and full path)
    fn find_app_path(&self, workspace: &Path, app: &str) -> Result<PathBuf, McpError> {
        let apps_dir = self.get_apps_dir(workspace);

        // Check if it's a direct path
        let direct_path = workspace.join(app);
        if direct_path.exists() && direct_path.join("CMakeLists.txt").exists() {
            return Ok(direct_path);
        }

        // Check in apps directory
        let app_path = apps_dir.join(app);
        if app_path.exists() && app_path.join("CMakeLists.txt").exists() {
            return Ok(app_path);
        }

        Err(McpError::invalid_params(
            format!("Application '{}' not found. Expected CMakeLists.txt in {} or {}",
                    app, direct_path.display(), app_path.display()),
            None,
        ))
    }
}

impl Default for ZephyrBuildToolHandler {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

#[tool_router]
impl ZephyrBuildToolHandler {
    // =============================================================================
    // Build Tools (5 tools)
    // =============================================================================

    #[tool(description = "List available Zephyr applications in the workspace")]
    async fn list_apps(&self, Parameters(args): Parameters<ListAppsArgs>) -> Result<CallToolResult, McpError> {
        debug!("Listing Zephyr applications");

        let workspace = self.find_workspace(args.workspace_path.as_deref())?;
        let apps_dir = self.get_apps_dir(&workspace);

        if !apps_dir.exists() {
            return Err(McpError::internal_error(
                format!("Apps directory not found: {}", apps_dir.display()),
                None,
            ));
        }

        let mut apps = Vec::new();

        // Scan apps directory for valid Zephyr applications
        let entries = std::fs::read_dir(&apps_dir).map_err(|e| {
            McpError::internal_error(format!("Failed to read apps directory: {}", e), None)
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let cmake_file = path.join("CMakeLists.txt");
                if cmake_file.exists() {
                    let name = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    let build_dir = path.join("build");
                    let has_build = build_dir.exists();

                    // Try to get board from build cache
                    let board = if has_build {
                        let cache_file = build_dir.join("CMakeCache.txt");
                        if cache_file.exists() {
                            std::fs::read_to_string(&cache_file)
                                .ok()
                                .and_then(|content| {
                                    content.lines()
                                        .find(|line| line.starts_with("BOARD:STRING="))
                                        .map(|line| line.replace("BOARD:STRING=", ""))
                                })
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let rel_path = path.strip_prefix(&workspace)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();

                    apps.push(AppInfo {
                        name,
                        path: rel_path,
                        has_build,
                        board,
                    });
                }
            }
        }

        apps.sort_by(|a, b| a.name.cmp(&b.name));

        let result = ListAppsResult { apps: apps.clone() };
        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        info!("Found {} Zephyr applications", apps.len());
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "List supported boards for Zephyr builds")]
    async fn list_boards(&self, Parameters(args): Parameters<ListBoardsArgs>) -> Result<CallToolResult, McpError> {
        debug!("Listing supported boards");

        let mut boards: Vec<BoardInfo> = COMMON_BOARDS
            .iter()
            .map(|(name, arch, vendor)| BoardInfo {
                name: name.to_string(),
                arch: arch.to_string(),
                vendor: Some(vendor.to_string()),
            })
            .collect();

        // If include_all is set, run west boards to get full list
        if args.include_all {
            if let Ok(workspace) = self.find_workspace(None) {
                let output = Command::new("west")
                    .args(["boards"])
                    .current_dir(&workspace)
                    .output()
                    .await;

                if let Ok(output) = output {
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        for line in stdout.lines() {
                            let board_name = line.trim();
                            if !board_name.is_empty() && !boards.iter().any(|b| b.name == board_name) {
                                boards.push(BoardInfo {
                                    name: board_name.to_string(),
                                    arch: "unknown".to_string(),
                                    vendor: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Apply filter if provided
        if let Some(filter) = &args.filter {
            let filter_lower = filter.to_lowercase();
            boards.retain(|b| {
                b.name.to_lowercase().contains(&filter_lower) ||
                b.arch.to_lowercase().contains(&filter_lower) ||
                b.vendor.as_ref().map(|v| v.to_lowercase().contains(&filter_lower)).unwrap_or(false)
            });
        }

        let result = ListBoardsResult { boards: boards.clone() };
        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        info!("Listed {} boards", boards.len());
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Build a Zephyr application for a target board")]
    async fn build(&self, Parameters(args): Parameters<BuildArgs>) -> Result<CallToolResult, McpError> {
        debug!("Building app '{}' for board '{}'", args.app, args.board);

        let workspace = self.find_workspace(args.workspace_path.as_deref())?;
        let app_path = self.find_app_path(&workspace, &args.app)?;

        // Build the command
        let mut cmd_args = vec![
            "build".to_string(),
            "-b".to_string(),
            args.board.clone(),
            app_path.to_string_lossy().to_string(),
        ];

        if args.pristine {
            cmd_args.push("--pristine".to_string());
        }

        if let Some(extra) = &args.extra_args {
            cmd_args.extend(extra.split_whitespace().map(String::from));
        }

        // Handle background builds
        if args.background {
            let build_id = uuid::Uuid::new_v4().to_string();

            let build_state = BuildState {
                status: BuildStatus::Running,
                output: String::new(),
                started_at: Instant::now(),
                app: args.app.clone(),
                board: args.board.clone(),
                artifact_path: None,
            };

            {
                let mut builds = self.builds.write().await;
                builds.insert(build_id.clone(), build_state);
            }

            // Spawn background task
            let builds = self.builds.clone();
            let build_id_clone = build_id.clone();
            let workspace_clone = workspace.clone();
            let app_clone = args.app.clone();

            tokio::spawn(async move {
                let start = Instant::now();
                let output = Command::new("west")
                    .args(&cmd_args)
                    .current_dir(&workspace_clone)
                    .output()
                    .await;

                let mut builds = builds.write().await;
                if let Some(state) = builds.get_mut(&build_id_clone) {
                    match output {
                        Ok(out) => {
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            let stderr = String::from_utf8_lossy(&out.stderr);
                            state.output = format!("{}\n{}", stdout, stderr);

                            if out.status.success() {
                                state.status = BuildStatus::Complete;
                                // Look for artifact
                                let artifact = workspace_clone
                                    .join(&app_clone)
                                    .join("build/zephyr/zephyr.elf");
                                if artifact.exists() {
                                    state.artifact_path = Some(artifact.to_string_lossy().to_string());
                                }
                            } else {
                                state.status = BuildStatus::Failed;
                            }
                        }
                        Err(e) => {
                            state.status = BuildStatus::Failed;
                            state.output = format!("Failed to execute west: {}", e);
                        }
                    }
                }
                info!("Background build {} completed in {:?}", build_id_clone, start.elapsed());
            });

            let result = BuildResult {
                success: true,
                build_id: Some(build_id.clone()),
                output: "Build started in background".to_string(),
                artifact_path: None,
                duration_ms: None,
            };

            let json = serde_json::to_string_pretty(&result).map_err(|e| {
                McpError::internal_error(format!("Serialization error: {}", e), None)
            })?;

            info!("Started background build: {}", build_id);
            return Ok(CallToolResult::success(vec![Content::text(json)]));
        }

        // Synchronous build
        let start = Instant::now();

        info!("Running: west {}", cmd_args.join(" "));

        let output = Command::new("west")
            .args(&cmd_args)
            .current_dir(&workspace)
            .output()
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Failed to execute west: {}", e), None)
            })?;

        let duration = start.elapsed();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined_output = format!("{}\n{}", stdout, stderr);

        let artifact_path = if output.status.success() {
            let artifact = app_path.join("build/zephyr/zephyr.elf");
            if artifact.exists() {
                Some(artifact.to_string_lossy().to_string())
            } else {
                None
            }
        } else {
            None
        };

        let result = BuildResult {
            success: output.status.success(),
            build_id: None,
            output: combined_output,
            artifact_path,
            duration_ms: Some(duration.as_millis() as u64),
        };

        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        if output.status.success() {
            info!("Build completed successfully in {:?}", duration);
        } else {
            error!("Build failed");
        }

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Clean build artifacts for a Zephyr application")]
    async fn clean(&self, Parameters(args): Parameters<CleanArgs>) -> Result<CallToolResult, McpError> {
        debug!("Cleaning build for app '{}'", args.app);

        let workspace = self.find_workspace(args.workspace_path.as_deref())?;
        let app_path = self.find_app_path(&workspace, &args.app)?;
        let build_dir = app_path.join("build");

        let result = if build_dir.exists() {
            match std::fs::remove_dir_all(&build_dir) {
                Ok(_) => CleanResult {
                    success: true,
                    message: format!("Removed build directory: {}", build_dir.display()),
                },
                Err(e) => CleanResult {
                    success: false,
                    message: format!("Failed to remove build directory: {}", e),
                },
            }
        } else {
            CleanResult {
                success: true,
                message: format!("Build directory does not exist: {}", build_dir.display()),
            }
        };

        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        info!("Clean result: {}", result.message);
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Check status of a background build")]
    async fn build_status(&self, Parameters(args): Parameters<BuildStatusArgs>) -> Result<CallToolResult, McpError> {
        debug!("Checking build status for '{}'", args.build_id);

        let builds = self.builds.read().await;

        let result = match builds.get(&args.build_id) {
            Some(state) => BuildStatusResult {
                status: match state.status {
                    BuildStatus::Running => "running".to_string(),
                    BuildStatus::Complete => "complete".to_string(),
                    BuildStatus::Failed => "failed".to_string(),
                },
                progress: if state.status == BuildStatus::Running {
                    Some(format!("Building {} for {} ({:?} elapsed)",
                                state.app, state.board, state.started_at.elapsed()))
                } else {
                    None
                },
                output: if state.status != BuildStatus::Running {
                    Some(state.output.clone())
                } else {
                    None
                },
                artifact_path: state.artifact_path.clone(),
                error: if state.status == BuildStatus::Failed {
                    Some("Build failed - see output for details".to_string())
                } else {
                    None
                },
            },
            None => {
                return Err(McpError::invalid_params(
                    format!("Build ID not found: {}", args.build_id),
                    None,
                ));
            }
        };

        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

#[tool_handler]
impl ServerHandler for ZephyrBuildToolHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Zephyr Build MCP Server - Build Zephyr RTOS applications. \
                 5 tools available: list_apps, list_boards, build, clean, build_status.".to_string()
            ),
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        info!("Zephyr Build MCP server initialized with 5 tools");
        Ok(self.get_info())
    }
}
