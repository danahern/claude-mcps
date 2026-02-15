//! Complete RMCP 0.3.2 implementation for Zephyr build MCP tools
//!
//! This implementation provides 6 build tools using west CLI subprocess calls

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

/// Zephyr build tool handler with all 6 tools
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
        if let Some(path) = &self.config.workspace_path {
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
        workspace.join(&self.config.apps_dir)
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
    // Build Tools (6 tools)
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

        // Per-app build directory so apps don't overwrite each other
        let build_dir = app_path.join("build");

        // Build the command
        let mut cmd_args = vec![
            "build".to_string(),
            "-b".to_string(),
            args.board.clone(),
            app_path.to_string_lossy().to_string(),
            "-d".to_string(),
            build_dir.to_string_lossy().to_string(),
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
            let app_path_clone = app_path.clone();

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
                                let artifact = app_path_clone
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

    #[tool(description = "Build all applications in the workspace for a target board")]
    async fn build_all(&self, Parameters(args): Parameters<BuildAllArgs>) -> Result<CallToolResult, McpError> {
        debug!("Building all apps for board '{}'", args.board);

        let workspace = self.find_workspace(args.workspace_path.as_deref())?;
        let apps_dir = self.get_apps_dir(&workspace);

        if !apps_dir.exists() {
            return Err(McpError::internal_error(
                format!("Apps directory not found: {}", apps_dir.display()),
                None,
            ));
        }

        // Discover apps (same logic as list_apps)
        let mut app_names = Vec::new();
        let entries = std::fs::read_dir(&apps_dir).map_err(|e| {
            McpError::internal_error(format!("Failed to read apps directory: {}", e), None)
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("CMakeLists.txt").exists() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    app_names.push(name.to_string());
                }
            }
        }
        app_names.sort();

        if app_names.is_empty() {
            return Err(McpError::internal_error(
                format!("No applications found in {}", apps_dir.display()),
                None,
            ));
        }

        info!("Building {} apps for board '{}'", app_names.len(), args.board);

        let total_start = Instant::now();
        let mut results = Vec::new();

        for app_name in &app_names {
            let app_path = apps_dir.join(app_name);
            let build_dir = app_path.join("build");

            let mut cmd_args = vec![
                "build".to_string(),
                "-b".to_string(),
                args.board.clone(),
                app_path.to_string_lossy().to_string(),
                "-d".to_string(),
                build_dir.to_string_lossy().to_string(),
            ];

            if args.pristine {
                cmd_args.push("--pristine".to_string());
            }

            info!("Building {}: west {}", app_name, cmd_args.join(" "));
            let start = Instant::now();

            let output = Command::new("west")
                .args(&cmd_args)
                .current_dir(&workspace)
                .output()
                .await;

            let duration_ms = start.elapsed().as_millis() as u64;

            match output {
                Ok(out) => {
                    if out.status.success() {
                        let artifact = app_path.join("build/zephyr/zephyr.elf");
                        let artifact_path = if artifact.exists() {
                            Some(artifact.to_string_lossy().to_string())
                        } else {
                            None
                        };
                        info!("  {} succeeded in {}ms", app_name, duration_ms);
                        results.push(AppBuildResult {
                            app: app_name.clone(),
                            success: true,
                            artifact_path,
                            error: None,
                            duration_ms,
                        });
                    } else {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        error!("  {} failed", app_name);
                        results.push(AppBuildResult {
                            app: app_name.clone(),
                            success: false,
                            artifact_path: None,
                            error: Some(stderr.to_string()),
                            duration_ms,
                        });
                    }
                }
                Err(e) => {
                    error!("  {} failed to execute: {}", app_name, e);
                    results.push(AppBuildResult {
                        app: app_name.clone(),
                        success: false,
                        artifact_path: None,
                        error: Some(format!("Failed to execute west: {}", e)),
                        duration_ms,
                    });
                }
            }
        }

        let total_duration_ms = total_start.elapsed().as_millis() as u64;
        let succeeded = results.iter().filter(|r| r.success).count();
        let failed = results.iter().filter(|r| !r.success).count();

        let result = BuildAllResult {
            total: results.len(),
            succeeded,
            failed,
            results,
            duration_ms: total_duration_ms,
        };

        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        info!("Build all complete: {}/{} succeeded in {}ms", succeeded, result.total, total_duration_ms);
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

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::handler::server::tool::Parameters;
    use std::fs;
    use tempfile::TempDir;

    /// Extract JSON text from a CallToolResult's first content element
    fn extract_json(result: &CallToolResult) -> serde_json::Value {
        let text = &result.content[0].as_text().expect("expected text content").text;
        serde_json::from_str(text).expect("expected valid JSON")
    }

    #[tokio::test]
    async fn test_list_boards_common() {
        let handler = ZephyrBuildToolHandler::default();
        let result = handler
            .list_boards(Parameters(ListBoardsArgs {
                filter: None,
                include_all: false,
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        let boards = parsed["boards"].as_array().unwrap();

        assert!(!boards.is_empty());
        let names: Vec<&str> = boards.iter().map(|b| b["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"nrf52840dk/nrf52840"));
        assert!(names.contains(&"native_sim"));
    }

    #[tokio::test]
    async fn test_list_boards_filter_nrf() {
        let handler = ZephyrBuildToolHandler::default();
        let result = handler
            .list_boards(Parameters(ListBoardsArgs {
                filter: Some("nrf".to_string()),
                include_all: false,
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        let boards = parsed["boards"].as_array().unwrap();

        assert!(!boards.is_empty());
        for board in boards {
            let name = board["name"].as_str().unwrap().to_lowercase();
            let vendor = board["vendor"].as_str().unwrap_or("").to_lowercase();
            assert!(
                name.contains("nrf") || vendor.contains("nrf"),
                "board {} should match 'nrf' filter",
                name
            );
        }
    }

    #[tokio::test]
    async fn test_list_boards_filter_no_match() {
        let handler = ZephyrBuildToolHandler::default();
        let result = handler
            .list_boards(Parameters(ListBoardsArgs {
                filter: Some("nonexistent_xyz".to_string()),
                include_all: false,
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert!(parsed["boards"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_list_apps_with_dummy_workspace() {
        let tmp = TempDir::new().unwrap();
        let apps_dir = tmp.path().join("zephyr-apps/apps");
        fs::create_dir_all(&apps_dir).unwrap();

        // Create a dummy app
        let app = apps_dir.join("my_app");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("CMakeLists.txt"), "project(my_app)\n").unwrap();

        // Create a dir without CMakeLists.txt (should be ignored)
        fs::create_dir_all(apps_dir.join("not_an_app")).unwrap();

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .list_apps(Parameters(ListAppsArgs { workspace_path: None }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        let apps = parsed["apps"].as_array().unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0]["name"].as_str().unwrap(), "my_app");
        assert!(!apps[0]["has_build"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_list_apps_empty_workspace() {
        let tmp = TempDir::new().unwrap();
        let apps_dir = tmp.path().join("zephyr-apps/apps");
        fs::create_dir_all(&apps_dir).unwrap();

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .list_apps(Parameters(ListAppsArgs { workspace_path: None }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert!(parsed["apps"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_list_apps_no_apps_dir() {
        let tmp = TempDir::new().unwrap();
        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .list_apps(Parameters(ListAppsArgs { workspace_path: None }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_build_status_unknown_id() {
        let handler = ZephyrBuildToolHandler::default();
        let result = handler
            .build_status(Parameters(BuildStatusArgs {
                build_id: "nonexistent-id".to_string(),
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_clean_nonexistent_workspace() {
        let handler = ZephyrBuildToolHandler::default();
        let result = handler
            .clean(Parameters(CleanArgs {
                app: "nonexistent_app".to_string(),
                workspace_path: Some("/tmp/nonexistent_workspace_xyz".to_string()),
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_build_all_no_apps_dir() {
        let tmp = TempDir::new().unwrap();
        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .build_all(Parameters(BuildAllArgs {
                board: "nrf52840dk/nrf52840".to_string(),
                pristine: false,
                workspace_path: None,
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_build_all_empty_workspace() {
        let tmp = TempDir::new().unwrap();
        let apps_dir = tmp.path().join("zephyr-apps/apps");
        fs::create_dir_all(&apps_dir).unwrap();

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .build_all(Parameters(BuildAllArgs {
                board: "nrf52840dk/nrf52840".to_string(),
                pristine: false,
                workspace_path: None,
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_clean_app_no_build_dir() {
        let tmp = TempDir::new().unwrap();
        let apps_dir = tmp.path().join("zephyr-apps/apps");
        let app = apps_dir.join("my_app");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("CMakeLists.txt"), "project(my_app)\n").unwrap();

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .clean(Parameters(CleanArgs {
                app: "my_app".to_string(),
                workspace_path: None,
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert!(parsed["success"].as_bool().unwrap());
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
                 6 tools available: list_apps, list_boards, build, build_all, clean, build_status.".to_string()
            ),
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        info!("Zephyr Build MCP server initialized with 6 tools");
        Ok(self.get_info())
    }
}
