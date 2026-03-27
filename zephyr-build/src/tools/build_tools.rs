//! Complete RMCP 0.3.2 implementation for Zephyr build and test MCP tools
//!
//! This implementation provides 9 tools: 6 build tools using west CLI subprocess calls
//! and 3 test tools using twister subprocess calls

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
use super::templates;
use crate::config::Config;

/// Get home directory path
fn dirs_path() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

/// Maximum output size in bytes before truncation kicks in
const MAX_OUTPUT_BYTES: usize = 8192;
/// Lines to keep from the start of output
const HEAD_LINES: usize = 50;
/// Lines to keep from the end of output
const TAIL_LINES: usize = 100;

/// Truncate build/test output to keep first HEAD_LINES and last TAIL_LINES,
/// replacing the middle with a truncation notice. Returns the input unchanged
/// if it's under MAX_OUTPUT_BYTES.
fn truncate_output(output: &str) -> String {
    if output.len() <= MAX_OUTPUT_BYTES {
        return output.to_string();
    }

    let lines: Vec<&str> = output.lines().collect();
    let total = lines.len();

    if total <= HEAD_LINES + TAIL_LINES {
        return output.to_string();
    }

    let head = &lines[..HEAD_LINES];
    let tail = &lines[total - TAIL_LINES..];
    let skipped = total - HEAD_LINES - TAIL_LINES;

    format!(
        "{}\n\n... [{} lines truncated] ...\n\n{}",
        head.join("\n"),
        skipped,
        tail.join("\n")
    )
}

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
    ("stm32mp157c_dk2", "arm", "ST"),
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

/// Test run state for background test runs
#[derive(Debug, Clone)]
pub struct TestState {
    pub status: TestRunStatus,
    pub output: String,
    pub started_at: Instant,
    pub board: String,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TestRunStatus {
    Running,
    Complete,
    Failed,
}

/// Zephyr build tool handler with all 9 tools
#[derive(Clone)]
pub struct ZephyrBuildToolHandler {
    #[allow(dead_code)]
    tool_router: ToolRouter<ZephyrBuildToolHandler>,
    config: Config,
    builds: Arc<RwLock<HashMap<String, BuildState>>>,
    tests: Arc<RwLock<HashMap<String, TestState>>>,
}

impl ZephyrBuildToolHandler {
    pub fn new(config: Config) -> Self {
        Self {
            tool_router: Self::tool_router(),
            config,
            builds: Arc::new(RwLock::new(HashMap::new())),
            tests: Arc::new(RwLock::new(HashMap::new())),
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

    /// Detect Zephyr SDK install path from cmake package registry or common locations
    fn find_zephyr_sdk() -> Option<PathBuf> {
        // 1. Check env var (already set by user)
        if let Ok(path) = std::env::var("ZEPHYR_SDK_INSTALL_DIR") {
            let p = PathBuf::from(&path);
            if p.join("sdk_version").exists() {
                return Some(p);
            }
        }

        // 2. Check cmake package registry (~/.cmake/packages/Zephyr-sdk/)
        if let Some(home) = dirs_path() {
            let registry = home.join(".cmake/packages/Zephyr-sdk");
            if registry.exists() {
                // Read all registration files, pick the newest SDK
                let mut sdk_paths: Vec<PathBuf> = Vec::new();
                if let Ok(entries) = std::fs::read_dir(&registry) {
                    for entry in entries.flatten() {
                        if let Ok(content) = std::fs::read_to_string(entry.path()) {
                            // Content is like "/path/to/zephyr-sdk-0.17.4/cmake"
                            let cmake_dir = PathBuf::from(content.trim());
                            if let Some(sdk_dir) = cmake_dir.parent() {
                                if sdk_dir.join("sdk_version").exists() {
                                    sdk_paths.push(sdk_dir.to_path_buf());
                                }
                            }
                        }
                    }
                }
                // Sort by path name descending to prefer newer versions
                sdk_paths.sort();
                if let Some(path) = sdk_paths.last() {
                    return Some(path.clone());
                }
            }
        }

        None
    }

    /// Create a `west` Command for the given workspace.
    /// In Docker mode: wraps as `docker exec -i <container> west`.
    /// In host mode: sets SDK env vars if not already present.
    fn west_cmd(&self, workspace: &Path) -> Command {
        if self.config.docker {
            let mut cmd = Command::new("docker");
            cmd.args(["exec", "-i", &self.config.docker_container, "west"])
               .current_dir(workspace);
            cmd
        } else {
            let mut cmd = Command::new("west");
            cmd.current_dir(workspace);
            // Set SDK env vars for host mode
            if std::env::var("ZEPHYR_TOOLCHAIN_VARIANT").is_err() {
                let sdk = self.config.sdk_path.clone()
                    .or_else(Self::find_zephyr_sdk);
                if let Some(sdk_path) = sdk {
                    cmd.env("ZEPHYR_TOOLCHAIN_VARIANT", "zephyr");
                    cmd.env("ZEPHYR_SDK_INSTALL_DIR", &sdk_path);
                    cmd.env("ZEPHYR_BASE", workspace.join("zephyr"));
                }
            }
            cmd
        }
    }

    /// Create a twister Command with Zephyr SDK environment set.
    /// In Docker mode: wraps as `docker exec -i <container> python3`.
    /// Strips pyenv shims from PATH to prevent stderr noise that corrupts
    /// cmake's JSON output (pyenv shim emits warnings that cmake merges
    /// into stdout via stderr=subprocess.STDOUT in verify-toolchain.cmake).
    fn twister_command(&self, workspace: &Path, cmd_args: &[String]) -> Command {
        if self.config.docker {
            let mut cmd = Command::new("docker");
            cmd.args(["exec", "-i", &self.config.docker_container, "python3"])
               .args(cmd_args)
               .current_dir(workspace);
            return cmd;
        }

        let mut cmd = Command::new("python3");
        cmd.args(cmd_args).current_dir(workspace);

        // Strip pyenv shims from PATH
        if let Ok(path) = std::env::var("PATH") {
            let cleaned: Vec<&str> = path
                .split(':')
                .filter(|p| !p.contains(".pyenv/shims"))
                .collect();
            cmd.env("PATH", cleaned.join(":"));
        }

        // Set SDK env vars if not already in environment
        if std::env::var("ZEPHYR_TOOLCHAIN_VARIANT").is_err() {
            let sdk = self.config.sdk_path.clone()
                .or_else(Self::find_zephyr_sdk);
            if let Some(sdk_path) = sdk {
                cmd.env("ZEPHYR_TOOLCHAIN_VARIANT", "zephyr");
                cmd.env("ZEPHYR_SDK_INSTALL_DIR", &sdk_path);
                cmd.env("ZEPHYR_BASE", workspace.join("zephyr"));
            }
        }

        cmd
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

    /// Get lib directory path (sibling of apps dir)
    fn get_lib_dir(&self, workspace: &Path) -> PathBuf {
        let apps_dir = self.get_apps_dir(workspace);
        apps_dir.parent().unwrap_or(&apps_dir).join("lib")
    }

    /// Get addons directory path (sibling of apps dir)
    fn get_addons_dir(&self, workspace: &Path) -> PathBuf {
        let apps_dir = self.get_apps_dir(workspace);
        apps_dir.parent().unwrap_or(&apps_dir).join("addons")
    }

    /// Read an addon manifest from addons/<name>.yml
    fn read_addon_manifest(&self, workspace: &Path, addon_name: &str) -> Result<AddonManifest, McpError> {
        let manifest_path = self.get_addons_dir(workspace).join(format!("{}.yml", addon_name));
        let content = std::fs::read_to_string(&manifest_path).map_err(|e| {
            McpError::invalid_params(
                format!("Cannot read addon manifest {}: {}", manifest_path.display(), e),
                None,
            )
        })?;
        serde_yaml::from_str(&content).map_err(|e| {
            McpError::internal_error(
                format!("Invalid addon manifest {}: {}", manifest_path.display(), e),
                None,
            )
        })
    }

    /// List all available addons by scanning addons/*.yml
    fn list_available_addons(&self, workspace: &Path) -> Vec<AddonInfo> {
        let addons_dir = self.get_addons_dir(workspace);
        let mut addons = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&addons_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("yml") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(manifest) = serde_yaml::from_str::<AddonManifest>(&content) {
                            addons.push(AddonInfo {
                                name: manifest.name,
                                description: manifest.description,
                                depends: manifest.depends,
                            });
                        }
                    }
                }
            }
        }

        addons.sort_by(|a, b| a.name.cmp(&b.name));
        addons
    }

    /// Read a library manifest from lib/<name>/manifest.yml
    fn read_library_manifest(&self, workspace: &Path, lib_name: &str) -> Result<LibraryManifest, McpError> {
        let manifest_path = self.get_lib_dir(workspace).join(lib_name).join("manifest.yml");
        let content = std::fs::read_to_string(&manifest_path).map_err(|e| {
            McpError::invalid_params(
                format!("Cannot read library manifest {}: {}", manifest_path.display(), e),
                None,
            )
        })?;
        serde_yaml::from_str(&content).map_err(|e| {
            McpError::internal_error(
                format!("Invalid library manifest {}: {}", manifest_path.display(), e),
                None,
            )
        })
    }

    /// Get the per-board build directory for an app.
    /// Converts board identifier slashes to underscores (e.g., "nrf52840dk/nrf52840" -> "nrf52840dk_nrf52840").
    fn build_dir_for_board(app_path: &Path, board: &str) -> PathBuf {
        let sanitized = board.replace('/', "_");
        app_path.join("build").join(sanitized)
    }

    /// Symlink compile_commands.json from the build directory to the app source root.
    /// Helps clangd find the compilation database and reduces false-positive diagnostics.
    fn symlink_compile_commands(app_path: &Path, build_dir: &Path) {
        let source = build_dir.join("compile_commands.json");
        let target = app_path.join("compile_commands.json");
        if source.exists() {
            // Remove existing symlink/file
            let _ = std::fs::remove_file(&target);
            #[cfg(unix)]
            {
                if let Err(e) = std::os::unix::fs::symlink(&source, &target) {
                    debug!("Failed to symlink compile_commands.json: {}", e);
                }
            }
        }
    }

    /// Read an app manifest from apps/<name>/manifest.yml (returns None if missing)
    fn read_app_manifest(app_path: &Path) -> Option<AppManifest> {
        let manifest_path = app_path.join("manifest.yml");
        let content = std::fs::read_to_string(&manifest_path).ok()?;
        serde_yaml::from_str(&content).ok()
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
    // Build Tools
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

                    // Scan per-board build subdirectories
                    let built_boards = if has_build {
                        let mut boards = Vec::new();
                        if let Ok(entries) = std::fs::read_dir(&build_dir) {
                            for entry in entries.flatten() {
                                let sub = entry.path();
                                if sub.is_dir() && sub.join("zephyr").exists() {
                                    if let Some(name) = sub.file_name().and_then(|n| n.to_str()) {
                                        // Convert back from sanitized name
                                        boards.push(name.to_string());
                                    }
                                }
                            }
                        }
                        boards.sort();
                        if boards.is_empty() { None } else { Some(boards) }
                    } else {
                        None
                    };

                    let rel_path = path.strip_prefix(&workspace)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();

                    let manifest = Self::read_app_manifest(&path);

                    apps.push(AppInfo {
                        name,
                        path: rel_path,
                        has_build,
                        built_boards,
                        description: manifest.as_ref().map(|m| m.description.clone()),
                        target_boards: manifest.as_ref().and_then(|m| {
                            if m.boards.is_empty() { None } else { Some(m.boards.clone()) }
                        }),
                        libraries: manifest.as_ref().and_then(|m| {
                            if m.libraries.is_empty() { None } else { Some(m.libraries.clone()) }
                        }),
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
                let output = self.west_cmd(&workspace)
                    .args(["boards"])
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

        // Per-board build directory so boards don't overwrite each other
        let build_dir = Self::build_dir_for_board(&app_path, &args.board);

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
            let build_dir_clone = build_dir.clone();
            let app_path_clone = app_path.clone();

            let docker = self.config.docker;
            let docker_container = self.config.docker_container.clone();

            tokio::spawn(async move {
                let start = Instant::now();
                let output = if docker {
                    Command::new("docker")
                        .args(["exec", "-i", &docker_container, "west"])
                        .args(&cmd_args)
                        .current_dir(&workspace_clone)
                        .output()
                        .await
                } else {
                    Command::new("west")
                        .args(&cmd_args)
                        .current_dir(&workspace_clone)
                        .output()
                        .await
                };

                let mut builds = builds.write().await;
                if let Some(state) = builds.get_mut(&build_id_clone) {
                    match output {
                        Ok(out) => {
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            let stderr = String::from_utf8_lossy(&out.stderr);
                            state.output = truncate_output(&format!("{}\n{}", stdout, stderr));

                            if out.status.success() {
                                state.status = BuildStatus::Complete;
                                ZephyrBuildToolHandler::symlink_compile_commands(&app_path_clone, &build_dir_clone);
                                // Look for artifact in per-board build dir
                                let artifact = build_dir_clone
                                    .join("zephyr/zephyr.elf");
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

        let output = self.west_cmd(&workspace)
            .args(&cmd_args)
            .output()
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Failed to execute west: {}", e), None)
            })?;

        let duration = start.elapsed();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined_output = truncate_output(&format!("{}\n{}", stdout, stderr));

        let artifact_path = if output.status.success() {
            Self::symlink_compile_commands(&app_path, &build_dir);
            let artifact = build_dir.join("zephyr/zephyr.elf");
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
            let build_dir = Self::build_dir_for_board(&app_path, &args.board);

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

            let output = self.west_cmd(&workspace)
                .args(&cmd_args)
                .output()
                .await;

            let duration_ms = start.elapsed().as_millis() as u64;

            match output {
                Ok(out) => {
                    if out.status.success() {
                        let artifact = build_dir.join("zephyr/zephyr.elf");
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
                            error: Some(truncate_output(&stderr)),
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

    #[tool(description = "Clean build artifacts for a Zephyr application. If board is specified, only that board's build is removed. Otherwise all board builds are removed.")]
    async fn clean(&self, Parameters(args): Parameters<CleanArgs>) -> Result<CallToolResult, McpError> {
        debug!("Cleaning build for app '{}'", args.app);

        let workspace = self.find_workspace(args.workspace_path.as_deref())?;
        let app_path = self.find_app_path(&workspace, &args.app)?;

        let (target_dir, label) = if let Some(board) = &args.board {
            (Self::build_dir_for_board(&app_path, board), format!("board '{}'", board))
        } else {
            (app_path.join("build"), "all boards".to_string())
        };

        let result = if target_dir.exists() {
            match std::fs::remove_dir_all(&target_dir) {
                Ok(_) => CleanResult {
                    success: true,
                    message: format!("Removed build artifacts for {}: {}", label, target_dir.display()),
                },
                Err(e) => CleanResult {
                    success: false,
                    message: format!("Failed to remove build directory: {}", e),
                },
            }
        } else {
            CleanResult {
                success: true,
                message: format!("No build artifacts for {}: {}", label, target_dir.display()),
            }
        };

        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        info!("Clean result: {}", result.message);
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "List available app templates and composable addons. Call this before create_app to see what templates and addons exist.")]
    async fn list_templates(&self, Parameters(_args): Parameters<ListTemplatesArgs>) -> Result<CallToolResult, McpError> {
        // Scan for addons — best-effort (may not have a workspace)
        let addons = match self.find_workspace(None) {
            Ok(workspace) => self.list_available_addons(&workspace),
            Err(_) => Vec::new(),
        };

        let result = ListTemplatesResult {
            templates: vec![
                TemplateInfo {
                    name: "core".to_string(),
                    description: "Foundation template with shell + crash debug. Includes RTT logging, \
                                  coredump detection, and device shell commands out of the box."
                        .to_string(),
                    default_libraries: vec!["crash_log".to_string(), "device_shell".to_string()],
                    files: vec![
                        "CMakeLists.txt".to_string(),
                        "prj.conf".to_string(),
                        "manifest.yml".to_string(),
                        "src/main.c".to_string(),
                    ],
                },
            ],
            addons,
        };

        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Create a new Zephyr application from a template")]
    async fn create_app(&self, Parameters(args): Parameters<CreateAppArgs>) -> Result<CallToolResult, McpError> {
        debug!("Creating app '{}'", args.name);

        // Validate name: lowercase alphanumeric + underscore
        if args.name.is_empty() || !args.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            return Err(McpError::invalid_params(
                format!("Invalid app name '{}': must be lowercase alphanumeric + underscore", args.name),
                None,
            ));
        }

        let template = args.template.as_deref().unwrap_or("core");
        if template != "core" {
            return Err(McpError::invalid_params(
                format!("Unknown template '{}'. Available: core", template),
                None,
            ));
        }

        let workspace = self.find_workspace(args.workspace_path.as_deref())?;
        let apps_dir = self.get_apps_dir(&workspace);
        let app_dir = apps_dir.join(&args.name);

        if app_dir.exists() {
            return Err(McpError::invalid_params(
                format!("App '{}' already exists at {}", args.name, app_dir.display()),
                None,
            ));
        }

        // Core template default libraries
        let mut all_libs = vec!["crash_log".to_string(), "device_shell".to_string()];
        if let Some(extra) = &args.libraries {
            for lib in extra {
                if !all_libs.contains(lib) {
                    all_libs.push(lib.clone());
                }
            }
        }

        // Resolve each name: library (overlay injection) or addon (code generation)
        let lib_dir = self.get_lib_dir(&workspace);
        let mut overlay_lines = Vec::new();
        let mut resolved_addons = Vec::new();
        let mut resolved_lib_names = Vec::new();

        for lib_name in &all_libs {
            // 1. Check lib/<name>/manifest.yml → library
            match self.read_library_manifest(&workspace, lib_name) {
                Ok(manifest) => {
                    for overlay in &manifest.default_overlays {
                        overlay_lines.push(format!(
                            "list(APPEND OVERLAY_CONFIG \"${{CMAKE_CURRENT_LIST_DIR}}/../../lib/{}/{}\")",
                            lib_name, overlay
                        ));
                    }
                    resolved_lib_names.push(lib_name.clone());
                    continue;
                }
                Err(_) => {
                    // Check if lib dir exists without manifest
                    if lib_dir.join(lib_name).exists() {
                        resolved_lib_names.push(lib_name.clone());
                        continue;
                    }
                }
            }

            // 2. Check addons/<name>.yml → addon
            match self.read_addon_manifest(&workspace, lib_name) {
                Ok(manifest) => {
                    resolved_lib_names.push(lib_name.clone());
                    resolved_addons.push(manifest);
                    continue;
                }
                Err(_) => {}
            }

            // 3. Neither found — error
            return Err(McpError::invalid_params(
                format!(
                    "'{}' not found. Checked lib/{}/manifest.yml and addons/{}.yml",
                    lib_name,
                    lib_name,
                    lib_name
                ),
                None,
            ));
        }

        // Check addon dependencies
        for addon in &resolved_addons {
            for dep in &addon.depends {
                if !resolved_lib_names.contains(dep) {
                    return Err(McpError::invalid_params(
                        format!(
                            "Addon '{}' depends on '{}' which is not included in libraries",
                            addon.name, dep
                        ),
                        None,
                    ));
                }
            }
        }

        let overlay_block = overlay_lines.join("\n");

        // Merge addon code sections
        let addon_code = templates::merge_addon_code(&resolved_addons, &args.name);

        let description = args.description.as_deref().unwrap_or(&args.name);
        let board = args.board.as_deref().unwrap_or("nrf52840dk/nrf52840");

        // Render templates
        let cmake_content = templates::render(templates::TEMPLATE_CMAKE, &[
            ("APP_NAME", &args.name),
            ("OVERLAY_LINES", &overlay_block),
        ]);

        let prj_conf_content = templates::render(templates::TEMPLATE_PRJ_CONF, &[
            ("ADDON_KCONFIG", &addon_code.kconfig),
        ]);

        let main_c_content = templates::render(templates::TEMPLATE_MAIN_C, &[
            ("APP_NAME", &args.name),
            ("ADDON_INCLUDES", &addon_code.includes),
            ("ADDON_GLOBALS", &addon_code.globals),
            ("ERR_DECL", &addon_code.err_decl),
            ("ADDON_INIT", &addon_code.init),
        ]);

        // Build manifest YAML lines
        let board_lines = format!("  - {}", board);
        let library_lines = all_libs.iter()
            .map(|l| format!("  - {}", l))
            .collect::<Vec<_>>()
            .join("\n");

        let manifest_content = templates::render(templates::TEMPLATE_MANIFEST, &[
            ("DESCRIPTION", description),
            ("BOARD_LINES", &board_lines),
            ("LIBRARY_LINES", &library_lines),
            ("TEMPLATE", template),
        ]);

        // Create directories and write files
        let src_dir = app_dir.join("src");
        std::fs::create_dir_all(&src_dir).map_err(|e| {
            McpError::internal_error(format!("Failed to create {}: {}", src_dir.display(), e), None)
        })?;

        let files = vec![
            ("CMakeLists.txt", cmake_content),
            ("prj.conf", prj_conf_content),
            ("manifest.yml", manifest_content),
            ("src/main.c", main_c_content),
        ];

        let mut created = Vec::new();
        for (rel_path, content) in &files {
            let full_path = app_dir.join(rel_path);
            std::fs::write(&full_path, content).map_err(|e| {
                McpError::internal_error(format!("Failed to write {}: {}", full_path.display(), e), None)
            })?;
            created.push(rel_path.to_string());
        }

        let rel_app_path = app_dir.strip_prefix(&workspace)
            .unwrap_or(&app_dir)
            .to_string_lossy()
            .to_string();

        let result = CreateAppResult {
            success: true,
            app_name: args.name.clone(),
            app_path: rel_app_path,
            files_created: created,
            message: format!("Created app '{}' from '{}' template", args.name, template),
        };

        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        info!("Created app '{}' with {} files", args.name, result.files_created.len());
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

    // =============================================================================
    // Test Tools (3 tools)
    // =============================================================================

    #[tool(description = "Run Zephyr tests using twister. Returns parsed results with pass/fail counts and failure details.")]
    async fn run_tests(&self, Parameters(args): Parameters<RunTestsArgs>) -> Result<CallToolResult, McpError> {
        debug!("Running tests for board '{}'", args.board);

        let workspace = self.find_workspace(args.workspace_path.as_deref())?;
        let twister_script = workspace.join("zephyr/scripts/twister");

        if !twister_script.exists() {
            return Err(McpError::internal_error(
                format!("Twister script not found at: {}", twister_script.display()),
                None,
            ));
        }

        // Resolve test path: default to lib/ directory under apps parent
        let apps_dir = self.get_apps_dir(&workspace);
        let apps_parent = apps_dir.parent().unwrap_or(&apps_dir);
        let test_path = match &args.path {
            Some(p) => apps_parent.join(p),
            None => apps_parent.join("lib"),
        };

        if !test_path.exists() {
            return Err(McpError::invalid_params(
                format!("Test path does not exist: {}", test_path.display()),
                None,
            ));
        }

        let test_id = uuid::Uuid::new_v4().to_string();
        let output_dir = workspace.join(".cache/twister").join(&test_id);

        // Build twister command args
        let mut cmd_args = vec![
            twister_script.to_string_lossy().to_string(),
            "-T".to_string(),
            test_path.to_string_lossy().to_string(),
            "-p".to_string(),
            args.board.clone(),
            "-O".to_string(),
            output_dir.to_string_lossy().to_string(),
            "--inline-logs".to_string(),
        ];

        if let Some(filter) = &args.filter {
            cmd_args.push("-k".to_string());
            cmd_args.push(filter.clone());
        }

        if let Some(extra) = &args.extra_args {
            cmd_args.extend(extra.split_whitespace().map(String::from));
        }

        if args.background {
            let test_state = TestState {
                status: TestRunStatus::Running,
                output: String::new(),
                started_at: Instant::now(),
                board: args.board.clone(),
                output_dir: output_dir.clone(),
            };

            {
                let mut tests = self.tests.write().await;
                tests.insert(test_id.clone(), test_state);
            }

            let tests = self.tests.clone();
            let test_id_clone = test_id.clone();
            let docker = self.config.docker;
            let docker_container = self.config.docker_container.clone();

            tokio::spawn(async move {
                let start = Instant::now();
                let output = if docker {
                    Command::new("docker")
                        .args(["exec", "-i", &docker_container, "python3"])
                        .args(&cmd_args)
                        .current_dir(&workspace)
                        .output()
                        .await
                } else {
                    Command::new("python3")
                        .args(&cmd_args)
                        .current_dir(&workspace)
                        .output()
                        .await
                };

                let mut tests = tests.write().await;
                if let Some(state) = tests.get_mut(&test_id_clone) {
                    match output {
                        Ok(out) => {
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            let stderr = String::from_utf8_lossy(&out.stderr);
                            state.output = truncate_output(&format!("{}\n{}", stdout, stderr));
                            // twister returns non-zero on test failures, which is not an execution error
                            state.status = TestRunStatus::Complete;
                        }
                        Err(e) => {
                            state.status = TestRunStatus::Failed;
                            state.output = format!("Failed to execute twister: {}", e);
                        }
                    }
                }
                info!("Background test run {} completed in {:?}", test_id_clone, start.elapsed());
            });

            let result = RunTestsResult {
                success: true,
                test_id: Some(test_id.clone()),
                summary: None,
                output: "Test run started in background".to_string(),
                duration_ms: 0,
            };

            let json = serde_json::to_string_pretty(&result).map_err(|e| {
                McpError::internal_error(format!("Serialization error: {}", e), None)
            })?;

            info!("Started background test run: {}", test_id);
            return Ok(CallToolResult::success(vec![Content::text(json)]));
        }

        // Synchronous test run
        let start = Instant::now();
        info!("Running: python3 {}", cmd_args.join(" "));

        let output = self.twister_command(&workspace, &cmd_args)
            .output()
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Failed to execute twister: {}", e), None)
            })?;

        let duration = start.elapsed();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined_output = truncate_output(&format!("{}\n{}", stdout, stderr));

        // Parse results from twister.json
        let summary = parse_twister_json(&output_dir)
            .map(|r| r.summary)
            .ok();

        let result = RunTestsResult {
            success: output.status.success(),
            test_id: Some(test_id),
            summary,
            output: combined_output,
            duration_ms: duration.as_millis() as u64,
        };

        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        if output.status.success() {
            info!("Tests completed successfully in {:?}", duration);
        } else {
            info!("Tests completed with failures in {:?}", duration);
        }

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Check status of a background test run")]
    async fn test_status(&self, Parameters(args): Parameters<TestStatusArgs>) -> Result<CallToolResult, McpError> {
        debug!("Checking test status for '{}'", args.test_id);

        let tests = self.tests.read().await;

        let result = match tests.get(&args.test_id) {
            Some(state) => {
                let summary = if state.status == TestRunStatus::Complete {
                    parse_twister_json(&state.output_dir).map(|r| r.summary).ok()
                } else {
                    None
                };

                TestStatusResult {
                    status: match state.status {
                        TestRunStatus::Running => "running".to_string(),
                        TestRunStatus::Complete => "complete".to_string(),
                        TestRunStatus::Failed => "failed".to_string(),
                    },
                    progress: if state.status == TestRunStatus::Running {
                        Some(format!("Testing on {} ({:?} elapsed)",
                                    state.board, state.started_at.elapsed()))
                    } else {
                        None
                    },
                    summary,
                    output: if state.status != TestRunStatus::Running {
                        Some(state.output.clone())
                    } else {
                        None
                    },
                    error: if state.status == TestRunStatus::Failed {
                        Some(state.output.clone())
                    } else {
                        None
                    },
                }
            }
            None => {
                return Err(McpError::invalid_params(
                    format!("Test ID not found: {}", args.test_id),
                    None,
                ));
            }
        };

        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Parse results from a completed test run. Returns structured test suites, failures, and summary.")]
    async fn test_results(&self, Parameters(args): Parameters<TestResultsArgs>) -> Result<CallToolResult, McpError> {
        debug!("Parsing test results");

        let output_dir = if let Some(test_id) = &args.test_id {
            // Look up from test state first
            let tests = self.tests.read().await;
            if let Some(state) = tests.get(test_id) {
                if state.status == TestRunStatus::Running {
                    return Err(McpError::invalid_params(
                        "Test run is still in progress".to_string(),
                        None,
                    ));
                }
                state.output_dir.clone()
            } else {
                // Fall back to conventional path
                let workspace = self.find_workspace(args.workspace_path.as_deref())?;
                workspace.join(".cache/twister").join(test_id)
            }
        } else if let Some(dir) = &args.results_dir {
            PathBuf::from(dir)
        } else {
            return Err(McpError::invalid_params(
                "Either test_id or results_dir is required".to_string(),
                None,
            ));
        };

        let result = parse_twister_json(&output_dir).map_err(|e| {
            McpError::internal_error(
                format!("Failed to parse test results from {}: {}", output_dir.display(), e),
                None,
            )
        })?;

        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

/// Parse twister.json output into structured results
fn parse_twister_json(output_dir: &Path) -> Result<TestResultsResult, String> {
    let json_path = output_dir.join("twister.json");
    let content = std::fs::read_to_string(&json_path)
        .map_err(|e| format!("Cannot read {}: {}", json_path.display(), e))?;

    let data: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid JSON in {}: {}", json_path.display(), e))?;

    let testsuites = data["testsuites"]
        .as_array()
        .ok_or_else(|| "Missing 'testsuites' array in twister.json".to_string())?;

    let mut summary = TestSummary {
        total: 0,
        passed: 0,
        failed: 0,
        skipped: 0,
        errors: 0,
    };

    let mut suites = Vec::new();
    let mut failures = Vec::new();

    for suite in testsuites {
        let name = suite["name"].as_str().unwrap_or("unknown").to_string();
        let platform = suite["platform"].as_str().unwrap_or("unknown").to_string();
        let status = suite["status"].as_str().unwrap_or("unknown").to_string();

        // Parse execution_time (twister outputs seconds as string like "2.50")
        let duration_ms = suite["execution_time"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .map(|s| (s * 1000.0) as u64)
            .unwrap_or(0);

        let used_ram = suite["used_ram"].as_u64();
        let used_rom = suite["used_rom"].as_u64();

        // Parse test cases
        let mut test_cases = Vec::new();
        if let Some(cases) = suite["testcases"].as_array() {
            for case in cases {
                let case_name = case["identifier"].as_str().unwrap_or("unknown").to_string();
                let case_status = case["status"].as_str().unwrap_or("unknown").to_string();
                let case_duration_ms = case["execution_time"]
                    .as_str()
                    .and_then(|s| s.parse::<f64>().ok())
                    .map(|s| (s * 1000.0) as u64)
                    .unwrap_or(0);
                let reason = case["reason"].as_str().map(|s| s.to_string());

                test_cases.push(TestCaseResult {
                    name: case_name,
                    status: case_status,
                    duration_ms: case_duration_ms,
                    reason,
                });
            }
        }

        // Count by status
        summary.total += 1;
        match status.as_str() {
            "passed" => summary.passed += 1,
            "failed" => summary.failed += 1,
            "error" => summary.errors += 1,
            "skipped" | "filtered" => summary.skipped += 1,
            _ => {}
        }

        // Collect failures
        if status == "failed" || status == "error" {
            let log = suite["log"].as_str().unwrap_or("").to_string();
            // Find first failing test case name if any
            let test_name = test_cases.iter()
                .find(|c| c.status == "failed" || c.status == "error")
                .map(|c| c.name.clone());
            failures.push(TestFailure {
                suite_name: name.clone(),
                test_name,
                platform: platform.clone(),
                log,
            });
        }

        suites.push(TestSuiteResult {
            name,
            platform,
            status,
            duration_ms,
            used_ram,
            used_rom,
            test_cases,
        });
    }

    Ok(TestResultsResult {
        summary,
        test_suites: suites,
        failures,
    })
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

    #[test]
    fn test_truncate_output_short() {
        let output = "line 1\nline 2\nline 3\n";
        assert_eq!(truncate_output(output), output);
    }

    #[test]
    fn test_truncate_output_long() {
        // Generate output that exceeds MAX_OUTPUT_BYTES with enough lines
        let lines: Vec<String> = (0..500)
            .map(|i| format!("line {:04}: {}", i, "x".repeat(50)))
            .collect();
        let output = lines.join("\n");
        assert!(output.len() > MAX_OUTPUT_BYTES);

        let truncated = truncate_output(&output);

        // Should be smaller
        assert!(truncated.len() < output.len());
        // Should contain head and tail
        assert!(truncated.contains("line 0000"));
        assert!(truncated.contains(&format!("line {:04}", HEAD_LINES - 1)));
        assert!(truncated.contains(&format!("line {:04}", 499)));
        // Should have truncation marker
        assert!(truncated.contains("lines truncated"));
        // Middle should be gone
        assert!(!truncated.contains(&format!("line {:04}", 200)));
    }

    #[test]
    fn test_truncate_output_under_byte_limit() {
        // Many short lines under byte limit — should not truncate
        let lines: Vec<String> = (0..200).map(|i| format!("L{}", i)).collect();
        let output = lines.join("\n");
        assert!(output.len() < MAX_OUTPUT_BYTES);
        assert_eq!(truncate_output(&output), output);
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
                board: None,
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
                board: None,
                workspace_path: None,
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert!(parsed["success"].as_bool().unwrap());
    }

    #[test]
    fn test_build_dir_for_board() {
        let app = PathBuf::from("/workspace/apps/my_app");
        let dir = ZephyrBuildToolHandler::build_dir_for_board(&app, "nrf52840dk/nrf52840");
        assert_eq!(dir, PathBuf::from("/workspace/apps/my_app/build/nrf52840dk_nrf52840"));
    }

    #[test]
    fn test_build_dir_for_board_with_multiple_slashes() {
        let app = PathBuf::from("/workspace/apps/my_app");
        let dir = ZephyrBuildToolHandler::build_dir_for_board(&app, "nrf54l15dk/nrf54l15/cpuapp");
        assert_eq!(dir, PathBuf::from("/workspace/apps/my_app/build/nrf54l15dk_nrf54l15_cpuapp"));
    }

    #[test]
    fn test_build_dir_for_board_simple() {
        let app = PathBuf::from("/workspace/apps/my_app");
        let dir = ZephyrBuildToolHandler::build_dir_for_board(&app, "qemu_cortex_m3");
        assert_eq!(dir, PathBuf::from("/workspace/apps/my_app/build/qemu_cortex_m3"));
    }

    #[tokio::test]
    async fn test_list_apps_with_per_board_builds() {
        let tmp = TempDir::new().unwrap();
        let apps_dir = tmp.path().join("zephyr-apps/apps");
        let app = apps_dir.join("my_app");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("CMakeLists.txt"), "project(my_app)\n").unwrap();

        // Create per-board build dirs with a zephyr/ subdir (artifact marker)
        fs::create_dir_all(app.join("build/nrf52840dk_nrf52840/zephyr")).unwrap();
        fs::create_dir_all(app.join("build/qemu_cortex_m3/zephyr")).unwrap();

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
        assert!(apps[0]["has_build"].as_bool().unwrap());
        let built: Vec<&str> = apps[0]["built_boards"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap()).collect();
        assert!(built.contains(&"nrf52840dk_nrf52840"));
        assert!(built.contains(&"qemu_cortex_m3"));
    }

    #[tokio::test]
    async fn test_clean_specific_board() {
        let tmp = TempDir::new().unwrap();
        let apps_dir = tmp.path().join("zephyr-apps/apps");
        let app = apps_dir.join("my_app");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("CMakeLists.txt"), "project(my_app)\n").unwrap();

        // Create two board builds
        fs::create_dir_all(app.join("build/nrf52840dk_nrf52840/zephyr")).unwrap();
        fs::create_dir_all(app.join("build/qemu_cortex_m3/zephyr")).unwrap();

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        // Clean only one board
        let result = handler
            .clean(Parameters(CleanArgs {
                app: "my_app".to_string(),
                board: Some("nrf52840dk/nrf52840".to_string()),
                workspace_path: None,
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert!(parsed["success"].as_bool().unwrap());

        // nrf board dir should be gone, qemu should remain
        assert!(!app.join("build/nrf52840dk_nrf52840").exists());
        assert!(app.join("build/qemu_cortex_m3").exists());
    }

    #[tokio::test]
    async fn test_clean_all_boards() {
        let tmp = TempDir::new().unwrap();
        let apps_dir = tmp.path().join("zephyr-apps/apps");
        let app = apps_dir.join("my_app");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("CMakeLists.txt"), "project(my_app)\n").unwrap();

        // Create two board builds
        fs::create_dir_all(app.join("build/nrf52840dk_nrf52840/zephyr")).unwrap();
        fs::create_dir_all(app.join("build/qemu_cortex_m3/zephyr")).unwrap();

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        // Clean all boards (board = None)
        let result = handler
            .clean(Parameters(CleanArgs {
                app: "my_app".to_string(),
                board: None,
                workspace_path: None,
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert!(parsed["success"].as_bool().unwrap());

        // Entire build dir should be gone
        assert!(!app.join("build").exists());
    }

    // =========================================================================
    // Test tool tests
    // =========================================================================

    /// Create a twister.json file with the given content
    fn write_twister_json(dir: &Path, content: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("twister.json"), content).unwrap();
    }

    /// Minimal twister.json with all passing tests
    const TWISTER_JSON_ALL_PASS: &str = r#"{
        "environment": {},
        "testsuites": [
            {
                "name": "lib.crash_log.unit_tests",
                "arch": "arm",
                "platform": "qemu_cortex_m3",
                "path": "lib/crash_log",
                "status": "passed",
                "runnable": true,
                "execution_time": "2.50",
                "build_time": "5.00",
                "used_ram": 8192,
                "used_rom": 32768,
                "testcases": [
                    {
                        "identifier": "test_crash_log_init",
                        "status": "passed",
                        "execution_time": "1.20"
                    },
                    {
                        "identifier": "test_crash_log_write",
                        "status": "passed",
                        "execution_time": "1.30"
                    }
                ]
            }
        ]
    }"#;

    /// Twister.json with mixed results (pass, fail, skip)
    const TWISTER_JSON_MIXED: &str = r#"{
        "environment": {},
        "testsuites": [
            {
                "name": "lib.crash_log.unit_tests",
                "arch": "arm",
                "platform": "qemu_cortex_m3",
                "path": "lib/crash_log",
                "status": "passed",
                "runnable": true,
                "execution_time": "2.50",
                "build_time": "5.00",
                "testcases": [
                    {
                        "identifier": "test_crash_log_init",
                        "status": "passed",
                        "execution_time": "2.50"
                    }
                ]
            },
            {
                "name": "lib.device_shell.tests",
                "arch": "arm",
                "platform": "qemu_cortex_m3",
                "path": "lib/device_shell",
                "status": "failed",
                "runnable": true,
                "execution_time": "3.00",
                "build_time": "4.00",
                "log": "FAIL: assertion failed at test_shell.c:42\nExpected 1, got 0",
                "testcases": [
                    {
                        "identifier": "test_shell_register",
                        "status": "passed",
                        "execution_time": "1.00"
                    },
                    {
                        "identifier": "test_shell_execute",
                        "status": "failed",
                        "execution_time": "2.00",
                        "reason": "assertion failed"
                    }
                ]
            },
            {
                "name": "lib.ble_utils.tests",
                "arch": "arm",
                "platform": "qemu_cortex_m3",
                "path": "lib/ble_utils",
                "status": "skipped",
                "runnable": false,
                "execution_time": "0.00",
                "build_time": "0.00",
                "testcases": []
            },
            {
                "name": "lib.sensor.tests",
                "arch": "arm",
                "platform": "qemu_cortex_m3",
                "path": "lib/sensor",
                "status": "error",
                "runnable": true,
                "execution_time": "0.00",
                "build_time": "1.50",
                "log": "CMake Error: could not find sensor.h",
                "testcases": []
            }
        ]
    }"#;

    #[test]
    fn test_parse_twister_json_all_pass() {
        let tmp = TempDir::new().unwrap();
        write_twister_json(tmp.path(), TWISTER_JSON_ALL_PASS);

        let result = parse_twister_json(tmp.path()).unwrap();

        assert_eq!(result.summary.total, 1);
        assert_eq!(result.summary.passed, 1);
        assert_eq!(result.summary.failed, 0);
        assert_eq!(result.summary.skipped, 0);
        assert_eq!(result.summary.errors, 0);
        assert!(result.failures.is_empty());

        assert_eq!(result.test_suites.len(), 1);
        let suite = &result.test_suites[0];
        assert_eq!(suite.name, "lib.crash_log.unit_tests");
        assert_eq!(suite.platform, "qemu_cortex_m3");
        assert_eq!(suite.status, "passed");
        assert_eq!(suite.duration_ms, 2500);
        assert_eq!(suite.used_ram, Some(8192));
        assert_eq!(suite.used_rom, Some(32768));

        assert_eq!(suite.test_cases.len(), 2);
        assert_eq!(suite.test_cases[0].name, "test_crash_log_init");
        assert_eq!(suite.test_cases[0].status, "passed");
        assert_eq!(suite.test_cases[0].duration_ms, 1200);
        assert_eq!(suite.test_cases[1].name, "test_crash_log_write");
        assert_eq!(suite.test_cases[1].duration_ms, 1300);
    }

    #[test]
    fn test_parse_twister_json_mixed_results() {
        let tmp = TempDir::new().unwrap();
        write_twister_json(tmp.path(), TWISTER_JSON_MIXED);

        let result = parse_twister_json(tmp.path()).unwrap();

        assert_eq!(result.summary.total, 4);
        assert_eq!(result.summary.passed, 1);
        assert_eq!(result.summary.failed, 1);
        assert_eq!(result.summary.skipped, 1);
        assert_eq!(result.summary.errors, 1);

        // Should have 2 failures (failed + error)
        assert_eq!(result.failures.len(), 2);

        let fail = &result.failures[0];
        assert_eq!(fail.suite_name, "lib.device_shell.tests");
        assert_eq!(fail.test_name.as_deref(), Some("test_shell_execute"));
        assert!(fail.log.contains("assertion failed"));

        let error = &result.failures[1];
        assert_eq!(error.suite_name, "lib.sensor.tests");
        assert_eq!(error.test_name, None); // no testcases in error suite
        assert!(error.log.contains("CMake Error"));
    }

    #[test]
    fn test_parse_twister_json_missing_file() {
        let tmp = TempDir::new().unwrap();
        let result = parse_twister_json(tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot read"));
    }

    #[test]
    fn test_parse_twister_json_invalid_json() {
        let tmp = TempDir::new().unwrap();
        write_twister_json(tmp.path(), "not valid json {{{");
        let result = parse_twister_json(tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid JSON"));
    }

    #[test]
    fn test_parse_twister_json_missing_testsuites() {
        let tmp = TempDir::new().unwrap();
        write_twister_json(tmp.path(), r#"{"environment": {}}"#);
        let result = parse_twister_json(tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing 'testsuites'"));
    }

    #[test]
    fn test_parse_twister_json_empty_testsuites() {
        let tmp = TempDir::new().unwrap();
        write_twister_json(tmp.path(), r#"{"testsuites": []}"#);

        let result = parse_twister_json(tmp.path()).unwrap();
        assert_eq!(result.summary.total, 0);
        assert!(result.test_suites.is_empty());
        assert!(result.failures.is_empty());
    }

    #[test]
    fn test_parse_twister_json_filtered_status() {
        let tmp = TempDir::new().unwrap();
        write_twister_json(tmp.path(), r#"{
            "testsuites": [{
                "name": "filtered_test",
                "platform": "qemu_cortex_m3",
                "status": "filtered",
                "execution_time": "0.00",
                "testcases": []
            }]
        }"#);

        let result = parse_twister_json(tmp.path()).unwrap();
        assert_eq!(result.summary.total, 1);
        assert_eq!(result.summary.skipped, 1);
        assert!(result.failures.is_empty());
    }

    #[tokio::test]
    async fn test_test_status_unknown_id() {
        let handler = ZephyrBuildToolHandler::default();
        let result = handler
            .test_status(Parameters(TestStatusArgs {
                test_id: "nonexistent-test-id".to_string(),
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_test_results_no_args() {
        let handler = ZephyrBuildToolHandler::default();
        let result = handler
            .test_results(Parameters(TestResultsArgs {
                test_id: None,
                results_dir: None,
                workspace_path: None,
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_test_results_from_dir() {
        let tmp = TempDir::new().unwrap();
        write_twister_json(tmp.path(), TWISTER_JSON_ALL_PASS);

        let handler = ZephyrBuildToolHandler::default();
        let result = handler
            .test_results(Parameters(TestResultsArgs {
                test_id: None,
                results_dir: Some(tmp.path().to_string_lossy().to_string()),
                workspace_path: None,
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert_eq!(parsed["summary"]["total"].as_u64().unwrap(), 1);
        assert_eq!(parsed["summary"]["passed"].as_u64().unwrap(), 1);
        assert_eq!(parsed["test_suites"].as_array().unwrap().len(), 1);
        assert!(parsed["failures"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_test_results_from_dir_with_failures() {
        let tmp = TempDir::new().unwrap();
        write_twister_json(tmp.path(), TWISTER_JSON_MIXED);

        let handler = ZephyrBuildToolHandler::default();
        let result = handler
            .test_results(Parameters(TestResultsArgs {
                test_id: None,
                results_dir: Some(tmp.path().to_string_lossy().to_string()),
                workspace_path: None,
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert_eq!(parsed["summary"]["total"].as_u64().unwrap(), 4);
        assert_eq!(parsed["summary"]["failed"].as_u64().unwrap(), 1);
        assert_eq!(parsed["summary"]["errors"].as_u64().unwrap(), 1);
        assert_eq!(parsed["failures"].as_array().unwrap().len(), 2);

        // Check failure details are included
        let failure = &parsed["failures"][0];
        assert_eq!(failure["suite_name"].as_str().unwrap(), "lib.device_shell.tests");
        assert!(failure["log"].as_str().unwrap().contains("assertion failed"));
    }

    #[tokio::test]
    async fn test_test_results_missing_dir() {
        let handler = ZephyrBuildToolHandler::default();
        let result = handler
            .test_results(Parameters(TestResultsArgs {
                test_id: None,
                results_dir: Some("/tmp/nonexistent_twister_output_xyz".to_string()),
                workspace_path: None,
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_test_status_with_stored_state() {
        let handler = ZephyrBuildToolHandler::default();
        let test_id = "test-state-check";
        let tmp = TempDir::new().unwrap();
        write_twister_json(tmp.path(), TWISTER_JSON_ALL_PASS);

        // Insert a completed test state
        {
            let mut tests = handler.tests.write().await;
            tests.insert(test_id.to_string(), TestState {
                status: TestRunStatus::Complete,
                output: "test output here".to_string(),
                started_at: Instant::now(),
                board: "qemu_cortex_m3".to_string(),
                output_dir: tmp.path().to_path_buf(),
            });
        }

        let result = handler
            .test_status(Parameters(TestStatusArgs {
                test_id: test_id.to_string(),
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert_eq!(parsed["status"].as_str().unwrap(), "complete");
        assert!(parsed["output"].as_str().unwrap().contains("test output"));
        assert!(parsed["summary"]["passed"].as_u64().unwrap() == 1);
        assert!(parsed["error"].is_null());
        assert!(parsed["progress"].is_null());
    }

    #[tokio::test]
    async fn test_test_status_running_state() {
        let handler = ZephyrBuildToolHandler::default();
        let test_id = "test-running";

        {
            let mut tests = handler.tests.write().await;
            tests.insert(test_id.to_string(), TestState {
                status: TestRunStatus::Running,
                output: String::new(),
                started_at: Instant::now(),
                board: "qemu_cortex_m3".to_string(),
                output_dir: PathBuf::from("/tmp/fake"),
            });
        }

        let result = handler
            .test_status(Parameters(TestStatusArgs {
                test_id: test_id.to_string(),
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert_eq!(parsed["status"].as_str().unwrap(), "running");
        assert!(parsed["progress"].as_str().unwrap().contains("qemu_cortex_m3"));
        assert!(parsed["output"].is_null());
        assert!(parsed["summary"].is_null());
    }

    #[tokio::test]
    async fn test_test_results_from_stored_running_state() {
        let handler = ZephyrBuildToolHandler::default();
        let test_id = "test-still-running";

        {
            let mut tests = handler.tests.write().await;
            tests.insert(test_id.to_string(), TestState {
                status: TestRunStatus::Running,
                output: String::new(),
                started_at: Instant::now(),
                board: "qemu_cortex_m3".to_string(),
                output_dir: PathBuf::from("/tmp/fake"),
            });
        }

        let result = handler
            .test_results(Parameters(TestResultsArgs {
                test_id: Some(test_id.to_string()),
                results_dir: None,
                workspace_path: None,
            }))
            .await;
        assert!(result.is_err()); // Should reject in-progress runs
    }

    #[test]
    fn test_parse_twister_json_sparse_fields() {
        // Real twister output may omit optional fields like used_ram, execution_time, testcases
        let tmp = TempDir::new().unwrap();
        write_twister_json(tmp.path(), r#"{
            "testsuites": [{
                "name": "minimal.test",
                "status": "passed"
            }]
        }"#);

        let result = parse_twister_json(tmp.path()).unwrap();
        assert_eq!(result.summary.total, 1);
        assert_eq!(result.summary.passed, 1);

        let suite = &result.test_suites[0];
        assert_eq!(suite.name, "minimal.test");
        assert_eq!(suite.platform, "unknown");
        assert_eq!(suite.duration_ms, 0);
        assert_eq!(suite.used_ram, None);
        assert_eq!(suite.used_rom, None);
        assert!(suite.test_cases.is_empty());
    }

    #[test]
    fn test_parse_twister_json_failure_reason_propagated() {
        // Verify that test case failure reasons are accessible in the parsed output
        let tmp = TempDir::new().unwrap();
        write_twister_json(tmp.path(), r#"{
            "testsuites": [{
                "name": "reason.test",
                "platform": "qemu_cortex_m3",
                "status": "failed",
                "execution_time": "1.00",
                "log": "full log output",
                "testcases": [{
                    "identifier": "test_with_reason",
                    "status": "failed",
                    "execution_time": "0.50",
                    "reason": "Expected 42, got 0"
                }]
            }]
        }"#);

        let result = parse_twister_json(tmp.path()).unwrap();

        // Failure should capture the reason from test case
        let case = &result.test_suites[0].test_cases[0];
        assert_eq!(case.reason.as_deref(), Some("Expected 42, got 0"));

        // And the suite-level failure should reference the failing test
        let failure = &result.failures[0];
        assert_eq!(failure.test_name.as_deref(), Some("test_with_reason"));
        assert_eq!(failure.log, "full log output");
    }

    #[test]
    fn test_parse_twister_json_multiple_platforms() {
        // Same test suite on different platforms — both should appear
        let tmp = TempDir::new().unwrap();
        write_twister_json(tmp.path(), r#"{
            "testsuites": [
                {"name": "my.test", "platform": "qemu_cortex_m3", "status": "passed", "execution_time": "1.00", "testcases": []},
                {"name": "my.test", "platform": "native_sim", "status": "failed", "execution_time": "2.00", "log": "segfault", "testcases": []}
            ]
        }"#);

        let result = parse_twister_json(tmp.path()).unwrap();
        assert_eq!(result.summary.total, 2);
        assert_eq!(result.summary.passed, 1);
        assert_eq!(result.summary.failed, 1);
        assert_eq!(result.test_suites.len(), 2);

        // Failure should be on native_sim platform
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].platform, "native_sim");
    }

    #[tokio::test]
    async fn test_test_status_failed_state_includes_output() {
        let handler = ZephyrBuildToolHandler::default();
        let test_id = "test-exec-failed";

        {
            let mut tests = handler.tests.write().await;
            tests.insert(test_id.to_string(), TestState {
                status: TestRunStatus::Failed,
                output: "Failed to execute twister: command not found".to_string(),
                started_at: Instant::now(),
                board: "qemu_cortex_m3".to_string(),
                output_dir: PathBuf::from("/tmp/fake"),
            });
        }

        let result = handler
            .test_status(Parameters(TestStatusArgs {
                test_id: test_id.to_string(),
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert_eq!(parsed["status"].as_str().unwrap(), "failed");
        // Error field should contain the failure message
        assert!(parsed["error"].as_str().unwrap().contains("command not found"));
        // Output should also be available
        assert!(parsed["output"].as_str().unwrap().contains("command not found"));
    }

    #[tokio::test]
    async fn test_run_tests_missing_twister_script() {
        let tmp = TempDir::new().unwrap();
        let apps_dir = tmp.path().join("zephyr-apps/apps");
        let lib_dir = tmp.path().join("zephyr-apps/lib");
        fs::create_dir_all(&apps_dir).unwrap();
        fs::create_dir_all(&lib_dir).unwrap();

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .run_tests(Parameters(RunTestsArgs {
                path: None,
                board: "qemu_cortex_m3".to_string(),
                filter: None,
                extra_args: None,
                background: false,
                workspace_path: None,
            }))
            .await;
        assert!(result.is_err());
        // Should fail because zephyr/scripts/twister doesn't exist
    }

    #[tokio::test]
    async fn test_run_tests_missing_test_path() {
        let tmp = TempDir::new().unwrap();
        let apps_dir = tmp.path().join("zephyr-apps/apps");
        fs::create_dir_all(&apps_dir).unwrap();
        // Create the twister script so we get past that check
        let twister_dir = tmp.path().join("zephyr/scripts");
        fs::create_dir_all(&twister_dir).unwrap();
        fs::write(twister_dir.join("twister"), "#!/bin/bash\n").unwrap();

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        // Default path (lib/) doesn't exist
        let result = handler
            .run_tests(Parameters(RunTestsArgs {
                path: None,
                board: "qemu_cortex_m3".to_string(),
                filter: None,
                extra_args: None,
                background: false,
                workspace_path: None,
            }))
            .await;
        assert!(result.is_err());
    }

    // =========================================================================
    // list_templates tests
    // =========================================================================

    #[tokio::test]
    async fn test_list_templates() {
        let handler = ZephyrBuildToolHandler::default();
        let result = handler
            .list_templates(Parameters(ListTemplatesArgs {}))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        let templates = parsed["templates"].as_array().unwrap();
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0]["name"].as_str().unwrap(), "core");
        let libs: Vec<&str> = templates[0]["default_libraries"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap()).collect();
        assert!(libs.contains(&"crash_log"));
        assert!(libs.contains(&"device_shell"));
        let files: Vec<&str> = templates[0]["files"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap()).collect();
        assert!(files.contains(&"src/main.c"));
    }

    // =========================================================================
    // create_app tests
    // =========================================================================

    /// Create a workspace with lib manifests for create_app tests
    fn setup_workspace_with_libs(tmp: &TempDir) {
        let apps_dir = tmp.path().join("zephyr-apps/apps");
        let lib_dir = tmp.path().join("zephyr-apps/lib");
        fs::create_dir_all(&apps_dir).unwrap();

        // crash_log lib with manifest
        let crash_log = lib_dir.join("crash_log/conf");
        fs::create_dir_all(&crash_log).unwrap();
        fs::write(lib_dir.join("crash_log/manifest.yml"), r#"
name: crash_log
description: "Boot-time coredump detection"
default_overlays:
  - conf/debug_base.conf
  - conf/debug_coredump_flash.conf
board_overlays: true
depends: []
"#).unwrap();

        // device_shell lib with manifest
        fs::create_dir_all(lib_dir.join("device_shell")).unwrap();
        fs::write(lib_dir.join("device_shell/manifest.yml"), r#"
name: device_shell
description: "Board info shell commands"
default_overlays:
  - device_shell.conf
board_overlays: false
depends: []
"#).unwrap();
    }

    #[tokio::test]
    async fn test_create_app_basic() {
        let tmp = TempDir::new().unwrap();
        setup_workspace_with_libs(&tmp);

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .create_app(Parameters(CreateAppArgs {
                name: "test_app".to_string(),
                template: None,
                board: Some("nrf52840dk/nrf52840".to_string()),
                libraries: None,
                description: Some("A test application".to_string()),
                workspace_path: None,
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert!(parsed["success"].as_bool().unwrap());
        assert_eq!(parsed["app_name"].as_str().unwrap(), "test_app");
        let files: Vec<&str> = parsed["files_created"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap()).collect();
        assert!(files.contains(&"CMakeLists.txt"));
        assert!(files.contains(&"prj.conf"));
        assert!(files.contains(&"manifest.yml"));
        assert!(files.contains(&"src/main.c"));

        // Verify files exist
        let app_dir = tmp.path().join("zephyr-apps/apps/test_app");
        assert!(app_dir.join("CMakeLists.txt").exists());
        assert!(app_dir.join("prj.conf").exists());
        assert!(app_dir.join("manifest.yml").exists());
        assert!(app_dir.join("src/main.c").exists());

        // Verify CMakeLists content includes overlays
        let cmake = fs::read_to_string(app_dir.join("CMakeLists.txt")).unwrap();
        assert!(cmake.contains("project(test_app)"));
        assert!(cmake.contains("crash_log/conf/debug_base.conf"));
        assert!(cmake.contains("device_shell/device_shell.conf"));

        // Verify main.c content
        let main_c = fs::read_to_string(app_dir.join("src/main.c")).unwrap();
        assert!(main_c.contains("LOG_MODULE_REGISTER(test_app"));
        assert!(main_c.contains("test_app booted"));

        // Verify manifest
        let manifest = fs::read_to_string(app_dir.join("manifest.yml")).unwrap();
        assert!(manifest.contains("A test application"));
        assert!(manifest.contains("nrf52840dk/nrf52840"));
        assert!(manifest.contains("crash_log"));
        assert!(manifest.contains("device_shell"));
    }

    #[tokio::test]
    async fn test_create_app_invalid_name() {
        let handler = ZephyrBuildToolHandler::default();

        for name in &["", "MyApp", "has-dash", "has space", "UPPER"] {
            let result = handler
                .create_app(Parameters(CreateAppArgs {
                    name: name.to_string(),
                    template: None,
                    board: None,
                    libraries: None,
                    description: None,
                    workspace_path: Some("/tmp/fake".to_string()),
                }))
                .await;
            assert!(result.is_err(), "Name '{}' should be rejected", name);
        }
    }

    #[tokio::test]
    async fn test_create_app_already_exists() {
        let tmp = TempDir::new().unwrap();
        setup_workspace_with_libs(&tmp);

        // Create existing app
        let app_dir = tmp.path().join("zephyr-apps/apps/existing_app");
        fs::create_dir_all(&app_dir).unwrap();

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .create_app(Parameters(CreateAppArgs {
                name: "existing_app".to_string(),
                template: None,
                board: None,
                libraries: None,
                description: None,
                workspace_path: None,
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_app_unknown_template() {
        let handler = ZephyrBuildToolHandler::default();
        let result = handler
            .create_app(Parameters(CreateAppArgs {
                name: "my_app".to_string(),
                template: Some("nonexistent".to_string()),
                board: None,
                libraries: None,
                description: None,
                workspace_path: Some("/tmp/fake".to_string()),
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_app_unknown_library() {
        let tmp = TempDir::new().unwrap();
        setup_workspace_with_libs(&tmp);

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .create_app(Parameters(CreateAppArgs {
                name: "my_app".to_string(),
                template: None,
                board: None,
                libraries: Some(vec!["nonexistent_lib".to_string()]),
                description: None,
                workspace_path: None,
            }))
            .await;
        assert!(result.is_err());
        // Error should mention both lib/ and addons/ paths
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("nonexistent_lib"));
    }

    // =========================================================================
    // addon tests
    // =========================================================================

    /// Create a workspace with lib manifests AND addon files for addon tests
    fn setup_workspace_with_addons(tmp: &TempDir) {
        setup_workspace_with_libs(tmp);

        let addons_dir = tmp.path().join("zephyr-apps/addons");
        fs::create_dir_all(&addons_dir).unwrap();

        fs::write(addons_dir.join("ble.yml"), r#"
name: ble
description: "BLE peripheral with NUS"
depends: []
kconfig: |
  # Bluetooth
  CONFIG_BT=y
  CONFIG_BT_PERIPHERAL=y
  CONFIG_BT_DEVICE_NAME="{{APP_NAME}}"
includes: |
  #include <zephyr/bluetooth/bluetooth.h>
  #include <zephyr/bluetooth/conn.h>
globals: |
  static struct bt_conn *current_conn;
init: |
  err = bt_enable(NULL);
  if (err) {
  	LOG_ERR("BT init failed: %d", err);
  }
"#).unwrap();

        fs::write(addons_dir.join("wifi.yml"), r#"
name: wifi
description: "WiFi station with DHCP"
depends: []
kconfig: |
  CONFIG_WIFI=y
  CONFIG_NETWORKING=y
includes: |
  #include <zephyr/net/wifi_mgmt.h>
globals: |
  static bool wifi_connected;
init: |
  err = wifi_connect();
"#).unwrap();
    }

    #[tokio::test]
    async fn test_create_app_with_addon() {
        let tmp = TempDir::new().unwrap();
        setup_workspace_with_addons(&tmp);

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .create_app(Parameters(CreateAppArgs {
                name: "test_ble".to_string(),
                template: None,
                board: Some("nrf52840dk/nrf52840".to_string()),
                libraries: Some(vec!["ble".to_string()]),
                description: None,
                workspace_path: None,
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert!(parsed["success"].as_bool().unwrap());

        let app_dir = tmp.path().join("zephyr-apps/apps/test_ble");

        // Verify main.c has BLE boilerplate
        let main_c = fs::read_to_string(app_dir.join("src/main.c")).unwrap();
        assert!(main_c.contains("#include <zephyr/bluetooth/bluetooth.h>"));
        assert!(main_c.contains("#include <zephyr/bluetooth/conn.h>"));
        assert!(main_c.contains("static struct bt_conn *current_conn;"));
        assert!(main_c.contains("int err;"));
        assert!(main_c.contains("err = bt_enable(NULL);"));

        // Verify prj.conf has BT Kconfig
        let prj_conf = fs::read_to_string(app_dir.join("prj.conf")).unwrap();
        assert!(prj_conf.contains("CONFIG_BT=y"));
        assert!(prj_conf.contains("CONFIG_BT_PERIPHERAL=y"));
        assert!(prj_conf.contains("CONFIG_BT_DEVICE_NAME=\"test_ble\""));

        // Base config still present
        assert!(prj_conf.contains("CONFIG_LOG=y"));
    }

    #[tokio::test]
    async fn test_create_app_with_multiple_addons() {
        let tmp = TempDir::new().unwrap();
        setup_workspace_with_addons(&tmp);

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .create_app(Parameters(CreateAppArgs {
                name: "test_combo".to_string(),
                template: None,
                board: None,
                libraries: Some(vec!["ble".to_string(), "wifi".to_string()]),
                description: None,
                workspace_path: None,
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert!(parsed["success"].as_bool().unwrap());

        let app_dir = tmp.path().join("zephyr-apps/apps/test_combo");
        let main_c = fs::read_to_string(app_dir.join("src/main.c")).unwrap();

        // Both addons' code should be present
        assert!(main_c.contains("#include <zephyr/bluetooth/bluetooth.h>"));
        assert!(main_c.contains("#include <zephyr/net/wifi_mgmt.h>"));
        assert!(main_c.contains("static struct bt_conn *current_conn;"));
        assert!(main_c.contains("static bool wifi_connected;"));
        assert!(main_c.contains("err = bt_enable(NULL);"));
        assert!(main_c.contains("err = wifi_connect();"));

        let prj_conf = fs::read_to_string(app_dir.join("prj.conf")).unwrap();
        assert!(prj_conf.contains("CONFIG_BT=y"));
        assert!(prj_conf.contains("CONFIG_WIFI=y"));
    }

    #[tokio::test]
    async fn test_create_app_no_addons_clean() {
        let tmp = TempDir::new().unwrap();
        setup_workspace_with_addons(&tmp);

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .create_app(Parameters(CreateAppArgs {
                name: "plain".to_string(),
                template: None,
                board: None,
                libraries: None,
                description: None,
                workspace_path: None,
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert!(parsed["success"].as_bool().unwrap());

        let app_dir = tmp.path().join("zephyr-apps/apps/plain");
        let main_c = fs::read_to_string(app_dir.join("src/main.c")).unwrap();

        // No unused int err;
        assert!(!main_c.contains("int err;"));
        // No addon includes
        assert!(!main_c.contains("#include <zephyr/bluetooth"));

        let prj_conf = fs::read_to_string(app_dir.join("prj.conf")).unwrap();
        assert!(!prj_conf.contains("CONFIG_BT=y"));
        // Should end cleanly
        assert!(prj_conf.contains("CONFIG_REBOOT=y"));
    }

    #[tokio::test]
    async fn test_create_app_library_still_works() {
        let tmp = TempDir::new().unwrap();
        setup_workspace_with_addons(&tmp);

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        // crash_log is a library, not an addon — should still generate overlay lines
        let result = handler
            .create_app(Parameters(CreateAppArgs {
                name: "lib_test".to_string(),
                template: None,
                board: None,
                libraries: None, // defaults include crash_log, device_shell
                description: None,
                workspace_path: None,
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert!(parsed["success"].as_bool().unwrap());

        let app_dir = tmp.path().join("zephyr-apps/apps/lib_test");
        let cmake = fs::read_to_string(app_dir.join("CMakeLists.txt")).unwrap();
        assert!(cmake.contains("crash_log/conf/debug_base.conf"));
    }

    #[tokio::test]
    async fn test_list_templates_includes_addons() {
        let tmp = TempDir::new().unwrap();
        setup_workspace_with_addons(&tmp);

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .list_templates(Parameters(ListTemplatesArgs {}))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        let addons = parsed["addons"].as_array().unwrap();
        assert!(addons.len() >= 2);

        let addon_names: Vec<&str> = addons.iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert!(addon_names.contains(&"ble"));
        assert!(addon_names.contains(&"wifi"));
    }

    #[tokio::test]
    async fn test_create_app_addon_dependency_missing() {
        let tmp = TempDir::new().unwrap();
        setup_workspace_with_addons(&tmp);

        // Add a tcp addon that depends on wifi
        let addons_dir = tmp.path().join("zephyr-apps/addons");
        fs::write(addons_dir.join("tcp.yml"), r#"
name: tcp
description: "TCP client"
depends: ["wifi"]
kconfig: |
  CONFIG_NET_SOCKETS=y
init: |
  LOG_INF("TCP ready");
"#).unwrap();

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        // Request tcp WITHOUT wifi — should fail
        let result = handler
            .create_app(Parameters(CreateAppArgs {
                name: "tcp_only".to_string(),
                template: None,
                board: None,
                libraries: Some(vec!["tcp".to_string()]),
                description: None,
                workspace_path: None,
            }))
            .await;
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("tcp"));
        assert!(err_msg.contains("wifi"));
        assert!(err_msg.contains("depends on"));
    }

    #[tokio::test]
    async fn test_create_app_addon_dependency_satisfied() {
        let tmp = TempDir::new().unwrap();
        setup_workspace_with_addons(&tmp);

        // Add a tcp addon that depends on wifi
        let addons_dir = tmp.path().join("zephyr-apps/addons");
        fs::write(addons_dir.join("tcp.yml"), r#"
name: tcp
description: "TCP client"
depends: ["wifi"]
kconfig: |
  CONFIG_NET_SOCKETS=y
init: |
  LOG_INF("TCP ready");
"#).unwrap();

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        // Request tcp WITH wifi — should succeed
        let result = handler
            .create_app(Parameters(CreateAppArgs {
                name: "tcp_wifi".to_string(),
                template: None,
                board: None,
                libraries: Some(vec!["wifi".to_string(), "tcp".to_string()]),
                description: None,
                workspace_path: None,
            }))
            .await;
        assert!(result.is_ok());

        let app_dir = tmp.path().join("zephyr-apps/apps/tcp_wifi");
        let prj_conf = fs::read_to_string(app_dir.join("prj.conf")).unwrap();
        assert!(prj_conf.contains("CONFIG_WIFI=y"));
        assert!(prj_conf.contains("CONFIG_NET_SOCKETS=y"));
    }

    #[tokio::test]
    async fn test_create_app_mixed_library_and_addon() {
        let tmp = TempDir::new().unwrap();
        setup_workspace_with_addons(&tmp);

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        // crash_log is a library, ble is an addon — both in same call
        let result = handler
            .create_app(Parameters(CreateAppArgs {
                name: "mixed_app".to_string(),
                template: None,
                board: None,
                libraries: Some(vec!["ble".to_string()]), // crash_log/device_shell are defaults
                description: None,
                workspace_path: None,
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert!(parsed["success"].as_bool().unwrap());

        let app_dir = tmp.path().join("zephyr-apps/apps/mixed_app");

        // Library overlay in CMakeLists.txt
        let cmake = fs::read_to_string(app_dir.join("CMakeLists.txt")).unwrap();
        assert!(cmake.contains("crash_log/conf/debug_base.conf"));

        // Addon code in main.c
        let main_c = fs::read_to_string(app_dir.join("src/main.c")).unwrap();
        assert!(main_c.contains("#include <zephyr/bluetooth/bluetooth.h>"));
        assert!(main_c.contains("err = bt_enable(NULL);"));
    }

    #[tokio::test]
    async fn test_list_templates_addons_sorted() {
        let tmp = TempDir::new().unwrap();
        setup_workspace_with_addons(&tmp);

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .list_templates(Parameters(ListTemplatesArgs {}))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        let addons = parsed["addons"].as_array().unwrap();
        let names: Vec<&str> = addons.iter().map(|a| a["name"].as_str().unwrap()).collect();

        // Should be alphabetically sorted
        let mut sorted_names = names.clone();
        sorted_names.sort();
        assert_eq!(names, sorted_names);
    }

    #[tokio::test]
    async fn test_list_templates_ignores_non_yml_files() {
        let tmp = TempDir::new().unwrap();
        setup_workspace_with_addons(&tmp);

        // Add a non-yml file in addons dir
        let addons_dir = tmp.path().join("zephyr-apps/addons");
        fs::write(addons_dir.join("README.md"), "# Addons\n").unwrap();
        fs::write(addons_dir.join("notes.txt"), "some notes").unwrap();

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .list_templates(Parameters(ListTemplatesArgs {}))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        let addons = parsed["addons"].as_array().unwrap();
        let names: Vec<&str> = addons.iter().map(|a| a["name"].as_str().unwrap()).collect();

        // Only yml files should be listed
        assert!(names.contains(&"ble"));
        assert!(names.contains(&"wifi"));
        assert!(!names.iter().any(|n| *n == "README" || *n == "notes"));
    }

    #[tokio::test]
    async fn test_create_app_addon_with_empty_string_fields() {
        let tmp = TempDir::new().unwrap();
        setup_workspace_with_libs(&tmp);

        let addons_dir = tmp.path().join("zephyr-apps/addons");
        fs::create_dir_all(&addons_dir).unwrap();

        // Addon with empty strings (not None) for some fields
        fs::write(addons_dir.join("minimal.yml"), r#"
name: minimal
description: "Minimal addon"
depends: []
kconfig: |
  CONFIG_MINIMAL=y
includes: ""
globals: ""
init: ""
"#).unwrap();

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .create_app(Parameters(CreateAppArgs {
                name: "min_app".to_string(),
                template: None,
                board: None,
                libraries: Some(vec!["minimal".to_string()]),
                description: None,
                workspace_path: None,
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert!(parsed["success"].as_bool().unwrap());

        let app_dir = tmp.path().join("zephyr-apps/apps/min_app");
        let main_c = fs::read_to_string(app_dir.join("src/main.c")).unwrap();

        // No int err; since init is empty
        assert!(!main_c.contains("int err;"));

        let prj_conf = fs::read_to_string(app_dir.join("prj.conf")).unwrap();
        assert!(prj_conf.contains("CONFIG_MINIMAL=y"));
    }

    #[test]
    fn test_addon_yaml_deserialization() {
        // Test that real addon YAML parses correctly
        let yaml = r#"
name: test_addon
description: "Test addon"
depends:
  - wifi
kconfig: |
  CONFIG_X=y
includes: |
  #include <test.h>
globals: |
  static int x;
init: |
  err = init();
"#;
        let manifest: super::super::types::AddonManifest =
            serde_yaml::from_str(yaml).unwrap();
        assert_eq!(manifest.name, "test_addon");
        assert_eq!(manifest.description, "Test addon");
        assert_eq!(manifest.depends, vec!["wifi"]);
        assert!(manifest.kconfig.as_ref().unwrap().contains("CONFIG_X=y"));
        assert!(manifest.includes.as_ref().unwrap().contains("#include <test.h>"));
        assert!(manifest.globals.as_ref().unwrap().contains("static int x;"));
        assert!(manifest.init.as_ref().unwrap().contains("err = init();"));
    }

    #[test]
    fn test_addon_yaml_minimal_deserialization() {
        // Addon with only required fields
        let yaml = r#"
name: bare
description: "Bare addon"
"#;
        let manifest: super::super::types::AddonManifest =
            serde_yaml::from_str(yaml).unwrap();
        assert_eq!(manifest.name, "bare");
        assert!(manifest.depends.is_empty());
        assert!(manifest.kconfig.is_none());
        assert!(manifest.includes.is_none());
        assert!(manifest.globals.is_none());
        assert!(manifest.init.is_none());
    }

    #[tokio::test]
    async fn test_create_app_nonexistent_mentions_both_paths() {
        let tmp = TempDir::new().unwrap();
        setup_workspace_with_addons(&tmp);

        let handler = ZephyrBuildToolHandler::new(Config {
            workspace_path: Some(tmp.path().to_path_buf()),
            apps_dir: "zephyr-apps/apps".to_string(),
        });

        let result = handler
            .create_app(Parameters(CreateAppArgs {
                name: "my_app".to_string(),
                template: None,
                board: None,
                libraries: Some(vec!["nonexistent".to_string()]),
                description: None,
                workspace_path: None,
            }))
            .await;
        assert!(result.is_err());
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(err_msg.contains("lib/nonexistent/manifest.yml"));
        assert!(err_msg.contains("addons/nonexistent.yml"));
    }

    // =========================================================================
    // list_apps manifest enrichment tests
    // =========================================================================

    #[tokio::test]
    async fn test_list_apps_with_manifest() {
        let tmp = TempDir::new().unwrap();
        let apps_dir = tmp.path().join("zephyr-apps/apps");
        let app = apps_dir.join("my_app");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("CMakeLists.txt"), "project(my_app)\n").unwrap();
        fs::write(app.join("manifest.yml"), r#"
description: "My test app"
boards:
  - nrf52840dk/nrf52840
libraries:
  - crash_log
template: core
"#).unwrap();

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
        assert_eq!(apps[0]["description"].as_str().unwrap(), "My test app");
        let boards: Vec<&str> = apps[0]["target_boards"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(boards, vec!["nrf52840dk/nrf52840"]);
        let libs: Vec<&str> = apps[0]["libraries"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(libs, vec!["crash_log"]);
    }

    #[tokio::test]
    async fn test_list_apps_without_manifest() {
        // Apps without manifest.yml should still work (backwards compatible)
        let tmp = TempDir::new().unwrap();
        let apps_dir = tmp.path().join("zephyr-apps/apps");
        let app = apps_dir.join("old_app");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("CMakeLists.txt"), "project(old_app)\n").unwrap();

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
        assert_eq!(apps[0]["name"].as_str().unwrap(), "old_app");
        // Manifest fields should not be present (skip_serializing_if)
        assert!(apps[0].get("description").is_none());
        assert!(apps[0].get("target_boards").is_none());
        assert!(apps[0].get("libraries").is_none());
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
                "Zephyr Build MCP Server - Build and test Zephyr RTOS applications. \
                 11 tools available: list_apps, list_boards, list_templates, build, build_all, \
                 clean, create_app, build_status, run_tests, test_status, test_results.".to_string()
            ),
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        info!("Zephyr Build MCP server initialized with 11 tools");
        Ok(self.get_info())
    }
}
