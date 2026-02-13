//! RMCP 0.3.2 implementation for ESP-IDF build MCP tools
//!
//! Provides 8 tools wrapping idf.py for ESP-IDF project management.

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

/// Supported ESP32 targets
const ESP_TARGETS: &[(&str, &str, &str)] = &[
    ("esp32", "xtensa", "ESP32 (dual-core Xtensa LX6, WiFi + BLE)"),
    ("esp32s2", "xtensa", "ESP32-S2 (single-core Xtensa LX7, WiFi)"),
    ("esp32s3", "xtensa", "ESP32-S3 (dual-core Xtensa LX7, WiFi + BLE)"),
    ("esp32c2", "riscv", "ESP32-C2 (single-core RISC-V, WiFi + BLE)"),
    ("esp32c3", "riscv", "ESP32-C3 (single-core RISC-V, WiFi + BLE)"),
    ("esp32c5", "riscv", "ESP32-C5 (single-core RISC-V, WiFi 6 + BLE)"),
    ("esp32c6", "riscv", "ESP32-C6 (single-core RISC-V, WiFi 6 + BLE + 802.15.4)"),
    ("esp32c61", "riscv", "ESP32-C61 (single-core RISC-V, WiFi 6 + BLE)"),
    ("esp32h2", "riscv", "ESP32-H2 (single-core RISC-V, BLE + 802.15.4)"),
    ("esp32p4", "riscv", "ESP32-P4 (dual-core RISC-V, high-performance)"),
];

/// Build state for background builds
#[derive(Debug, Clone)]
pub struct BuildState {
    pub status: BuildStatus,
    pub output: String,
    pub started_at: Instant,
    pub project: String,
    pub artifact_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BuildStatus {
    Running,
    Complete,
    Failed,
}

/// ESP-IDF build tool handler with all 8 tools
#[derive(Clone)]
pub struct EspIdfBuildToolHandler {
    #[allow(dead_code)]
    tool_router: ToolRouter<EspIdfBuildToolHandler>,
    config: Config,
    builds: Arc<RwLock<HashMap<String, BuildState>>>,
    /// Cached environment from export.sh
    idf_env: Arc<RwLock<Option<HashMap<String, String>>>>,
}

impl EspIdfBuildToolHandler {
    pub fn new(config: Config) -> Self {
        Self {
            tool_router: Self::tool_router(),
            config,
            builds: Arc::new(RwLock::new(HashMap::new())),
            idf_env: Arc::new(RwLock::new(None)),
        }
    }

    /// Find IDF_PATH from config, env, or common locations
    fn find_idf_path(&self) -> Result<PathBuf, McpError> {
        // 1. Config
        if let Some(path) = &self.config.idf_path {
            if path.exists() {
                return Ok(path.clone());
            }
        }

        // 2. IDF_PATH env var
        if let Ok(path) = std::env::var("IDF_PATH") {
            let p = PathBuf::from(&path);
            if p.exists() {
                return Ok(p);
            }
        }

        // 3. Common locations
        let home = std::env::var("HOME").unwrap_or_default();
        let candidates = [
            format!("{}/esp/esp-idf", home),
            "/opt/esp-idf".to_string(),
            format!("{}/esp/v5.4/esp-idf", home),
        ];

        for candidate in &candidates {
            let p = PathBuf::from(candidate);
            if p.exists() && p.join("tools").exists() {
                return Ok(p);
            }
        }

        Err(McpError::internal_error(
            "Could not find ESP-IDF. Set --idf-path, IDF_PATH env var, or install to ~/esp/esp-idf".to_string(),
            None,
        ))
    }

    /// Get or initialize the cached IDF environment.
    /// Sources export.sh once and caches all environment variables.
    async fn get_idf_env(&self) -> Result<HashMap<String, String>, McpError> {
        // Check cache first
        {
            let cache = self.idf_env.read().await;
            if let Some(env) = cache.as_ref() {
                return Ok(env.clone());
            }
        }

        let idf_path = self.find_idf_path()?;
        let export_sh = idf_path.join("export.sh");

        if !export_sh.exists() {
            return Err(McpError::internal_error(
                format!("export.sh not found at {}", export_sh.display()),
                None,
            ));
        }

        info!("Sourcing ESP-IDF environment from {}", export_sh.display());

        // Source export.sh and capture the resulting environment
        let output = Command::new("bash")
            .args([
                "-c",
                &format!(
                    "source '{}' >/dev/null 2>&1 && env -0",
                    export_sh.display()
                ),
            ])
            .env("IDF_PATH", &idf_path)
            .output()
            .await
            .map_err(|e| {
                McpError::internal_error(
                    format!("Failed to source export.sh: {}", e),
                    None,
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(McpError::internal_error(
                format!("export.sh failed: {}", stderr),
                None,
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut env_map = HashMap::new();

        for entry in stdout.split('\0') {
            if let Some((key, value)) = entry.split_once('=') {
                env_map.insert(key.to_string(), value.to_string());
            }
        }

        if env_map.is_empty() {
            return Err(McpError::internal_error(
                "No environment variables captured from export.sh".to_string(),
                None,
            ));
        }

        info!("Cached {} environment variables from ESP-IDF", env_map.len());

        // Cache it
        let mut cache = self.idf_env.write().await;
        *cache = Some(env_map.clone());

        Ok(env_map)
    }

    /// Run idf.py with the cached IDF environment
    async fn run_idf_py(
        &self,
        project_dir: &Path,
        args: &[&str],
    ) -> Result<std::process::Output, McpError> {
        let env = self.get_idf_env().await?;

        debug!("Running: idf.py {} in {}", args.join(" "), project_dir.display());

        Command::new("python3")
            .arg(env.get("IDF_PATH")
                .map(|p| format!("{}/tools/idf.py", p))
                .unwrap_or_else(|| "idf.py".to_string()))
            .args(args)
            .current_dir(project_dir)
            .envs(&env)
            .output()
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Failed to execute idf.py: {}", e), None)
            })
    }

    /// Find projects directory from args override, config, or default
    fn find_projects_dir(&self, override_path: Option<&str>) -> Result<PathBuf, McpError> {
        if let Some(path) = override_path {
            let p = PathBuf::from(path);
            if p.exists() {
                return Ok(p);
            }
            return Err(McpError::invalid_params(
                format!("Projects directory does not exist: {}", path),
                None,
            ));
        }

        if let Some(path) = &self.config.projects_dir {
            if path.exists() {
                return Ok(path.clone());
            }
        }

        Err(McpError::invalid_params(
            "No projects directory configured. Set --projects-dir or pass projects_dir argument.".to_string(),
            None,
        ))
    }

    /// Find project path (handles both name and full path)
    fn find_project_path(
        &self,
        projects_dir: &Path,
        project: &str,
    ) -> Result<PathBuf, McpError> {
        // Check if it's an absolute path
        let as_path = PathBuf::from(project);
        if as_path.is_absolute() && as_path.exists() && as_path.join("CMakeLists.txt").exists() {
            return Ok(as_path);
        }

        // Check in projects directory
        let project_path = projects_dir.join(project);
        if project_path.exists() && project_path.join("CMakeLists.txt").exists() {
            return Ok(project_path);
        }

        // Try recursive search one level deep (e.g., examples/esp32-p4-eye/factory)
        if let Ok(entries) = std::fs::read_dir(projects_dir) {
            for entry in entries.flatten() {
                let sub = entry.path().join(project);
                if sub.exists() && sub.join("CMakeLists.txt").exists() {
                    return Ok(sub);
                }
            }
        }

        Err(McpError::invalid_params(
            format!(
                "Project '{}' not found. Expected CMakeLists.txt in {} or subdirectories",
                project,
                projects_dir.display()
            ),
            None,
        ))
    }

    /// Get the default serial port
    fn get_port(&self, override_port: Option<&str>) -> Option<String> {
        override_port
            .map(|s| s.to_string())
            .or_else(|| self.config.default_port.clone())
    }
}

impl Default for EspIdfBuildToolHandler {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

#[tool_router]
impl EspIdfBuildToolHandler {
    #[tool(description = "List available ESP-IDF projects in the projects directory. Scans for directories containing CMakeLists.txt with a project() call.")]
    async fn list_projects(
        &self,
        Parameters(args): Parameters<ListProjectsArgs>,
    ) -> Result<CallToolResult, McpError> {
        debug!("Listing ESP-IDF projects");

        let projects_dir = self.find_projects_dir(args.projects_dir.as_deref())?;
        let mut projects = Vec::new();

        scan_projects(&projects_dir, &projects_dir, &mut projects);

        projects.sort_by(|a, b| a.name.cmp(&b.name));

        let result = ListProjectsResult {
            projects: projects.clone(),
        };
        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        info!("Found {} ESP-IDF projects", projects.len());
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "List supported ESP32 target chips")]
    async fn list_targets(
        &self,
        Parameters(_args): Parameters<ListTargetsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let targets: Vec<TargetInfo> = ESP_TARGETS
            .iter()
            .map(|(name, arch, desc)| TargetInfo {
                name: name.to_string(),
                arch: arch.to_string(),
                description: desc.to_string(),
            })
            .collect();

        let result = ListTargetsResult {
            targets: targets.clone(),
        };
        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        info!("Listed {} ESP32 targets", targets.len());
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Set the target chip for an ESP-IDF project. This runs 'idf.py set-target' which configures the project's sdkconfig for the specified chip.")]
    async fn set_target(
        &self,
        Parameters(args): Parameters<SetTargetArgs>,
    ) -> Result<CallToolResult, McpError> {
        debug!("Setting target '{}' for project '{}'", args.target, args.project);

        let projects_dir = self.find_projects_dir(args.projects_dir.as_deref())?;
        let project_path = self.find_project_path(&projects_dir, &args.project)?;

        let output = self
            .run_idf_py(&project_path, &["set-target", &args.target])
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        let result = SetTargetResult {
            success: output.status.success(),
            output: combined,
        };

        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        if output.status.success() {
            info!("Set target to '{}' for project '{}'", args.target, args.project);
        } else {
            error!("Failed to set target for project '{}'", args.project);
        }

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Build an ESP-IDF project. Runs 'idf.py build'. Target must be set first via set_target. Supports background builds.")]
    async fn build(
        &self,
        Parameters(args): Parameters<BuildArgs>,
    ) -> Result<CallToolResult, McpError> {
        debug!("Building project '{}'", args.project);

        let projects_dir = self.find_projects_dir(args.projects_dir.as_deref())?;
        let project_path = self.find_project_path(&projects_dir, &args.project)?;

        if args.background {
            let build_id = uuid::Uuid::new_v4().to_string();

            let build_state = BuildState {
                status: BuildStatus::Running,
                output: String::new(),
                started_at: Instant::now(),
                project: args.project.clone(),
                artifact_path: None,
            };

            {
                let mut builds = self.builds.write().await;
                builds.insert(build_id.clone(), build_state);
            }

            // Spawn background task
            let builds = self.builds.clone();
            let build_id_clone = build_id.clone();
            let project_path_clone = project_path.clone();

            // Pre-fetch the IDF env before spawning
            let env = self.get_idf_env().await?;

            tokio::spawn(async move {
                let start = Instant::now();

                let idf_py = env
                    .get("IDF_PATH")
                    .map(|p| format!("{}/tools/idf.py", p))
                    .unwrap_or_else(|| "idf.py".to_string());

                let output = Command::new("python3")
                    .arg(&idf_py)
                    .arg("build")
                    .current_dir(&project_path_clone)
                    .envs(&env)
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
                                let artifact = project_path_clone.join("build").join("flasher_args.json");
                                if artifact.exists() {
                                    state.artifact_path =
                                        Some(project_path_clone.join("build").to_string_lossy().to_string());
                                }
                            } else {
                                state.status = BuildStatus::Failed;
                            }
                        }
                        Err(e) => {
                            state.status = BuildStatus::Failed;
                            state.output = format!("Failed to execute idf.py: {}", e);
                        }
                    }
                }
                info!(
                    "Background build {} completed in {:?}",
                    build_id_clone,
                    start.elapsed()
                );
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

        let output = self.run_idf_py(&project_path, &["build"]).await?;

        let duration = start.elapsed();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        let artifact_path = if output.status.success() {
            let build_dir = project_path.join("build");
            if build_dir.join("flasher_args.json").exists() {
                Some(build_dir.to_string_lossy().to_string())
            } else {
                None
            }
        } else {
            None
        };

        let result = BuildResult {
            success: output.status.success(),
            build_id: None,
            output: combined,
            artifact_path,
            duration_ms: Some(duration.as_millis() as u64),
        };

        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        if output.status.success() {
            info!("Build completed successfully in {:?}", duration);
        } else {
            error!("Build failed for project '{}'", args.project);
        }

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Flash an ESP-IDF project to a connected device. Runs 'idf.py flash' which handles multi-segment flashing (bootloader + partition table + app) automatically.")]
    async fn flash(
        &self,
        Parameters(args): Parameters<FlashArgs>,
    ) -> Result<CallToolResult, McpError> {
        debug!("Flashing project '{}'", args.project);

        let projects_dir = self.find_projects_dir(args.projects_dir.as_deref())?;
        let project_path = self.find_project_path(&projects_dir, &args.project)?;

        let mut idf_args: Vec<&str> = Vec::new();

        // Port argument must come before the flash command
        let port_string;
        if let Some(port) = self.get_port(args.port.as_deref()) {
            port_string = port;
            idf_args.extend_from_slice(&["-p", &port_string]);
        }

        let baud_string;
        if let Some(baud) = args.baud {
            baud_string = baud.to_string();
            idf_args.extend_from_slice(&["-b", &baud_string]);
        }

        idf_args.push("flash");

        let output = self.run_idf_py(&project_path, &idf_args).await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        let result = FlashResult {
            success: output.status.success(),
            output: combined,
        };

        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        if output.status.success() {
            info!("Flash completed for project '{}'", args.project);
        } else {
            error!("Flash failed for project '{}'", args.project);
        }

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Clean build artifacts for an ESP-IDF project. Runs 'idf.py fullclean' to remove the entire build directory.")]
    async fn clean(
        &self,
        Parameters(args): Parameters<CleanArgs>,
    ) -> Result<CallToolResult, McpError> {
        debug!("Cleaning project '{}'", args.project);

        let projects_dir = self.find_projects_dir(args.projects_dir.as_deref())?;
        let project_path = self.find_project_path(&projects_dir, &args.project)?;

        let output = self.run_idf_py(&project_path, &["fullclean"]).await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        let result = CleanResult {
            success: output.status.success(),
            message: combined,
        };

        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        info!("Clean result for '{}': success={}", args.project, result.success);
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Check status of a background build")]
    async fn build_status(
        &self,
        Parameters(args): Parameters<BuildStatusArgs>,
    ) -> Result<CallToolResult, McpError> {
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
                    Some(format!(
                        "Building {} ({:?} elapsed)",
                        state.project,
                        state.started_at.elapsed()
                    ))
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

    #[tool(description = "Monitor serial output from an ESP32 device. Runs 'idf.py monitor' for a specified duration, capturing output. Useful for checking boot messages and runtime logs.")]
    async fn monitor(
        &self,
        Parameters(args): Parameters<MonitorArgs>,
    ) -> Result<CallToolResult, McpError> {
        debug!("Monitoring project '{}' for {}s", args.project, args.duration_seconds);

        let projects_dir = self.find_projects_dir(args.projects_dir.as_deref())?;
        let project_path = self.find_project_path(&projects_dir, &args.project)?;
        let env = self.get_idf_env().await?;

        let idf_py = env
            .get("IDF_PATH")
            .map(|p| format!("{}/tools/idf.py", p))
            .unwrap_or_else(|| "idf.py".to_string());

        let mut cmd = Command::new("python3");
        cmd.arg(&idf_py);

        if let Some(port) = self.get_port(args.port.as_deref()) {
            cmd.args(["-p", &port]);
        }

        cmd.arg("monitor")
            .arg("--no-reset")
            .current_dir(&project_path)
            .envs(&env)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            McpError::internal_error(format!("Failed to start monitor: {}", e), None)
        })?;

        // Wait for specified duration, then kill
        let duration = std::time::Duration::from_secs(args.duration_seconds);
        tokio::time::sleep(duration).await;

        let _ = child.kill().await;

        let output = child.wait_with_output().await.map_err(|e| {
            McpError::internal_error(format!("Failed to read monitor output: {}", e), None)
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = if stdout.is_empty() {
            stderr.to_string()
        } else {
            format!("{}\n{}", stdout, stderr)
        };

        let result = MonitorResult {
            success: true,
            output: combined,
            duration_seconds: args.duration_seconds,
        };

        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("Serialization error: {}", e), None)
        })?;

        info!("Monitor captured {}s of output for '{}'", args.duration_seconds, args.project);
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

/// Recursively scan for ESP-IDF projects (directories containing CMakeLists.txt with project())
fn scan_projects(dir: &Path, base_dir: &Path, projects: &mut Vec<ProjectInfo>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Skip hidden dirs and build dirs
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "build" || name == "managed_components" {
            continue;
        }

        let cmake = path.join("CMakeLists.txt");
        if cmake.exists() {
            // Check if it has a project() call (ESP-IDF project marker)
            if let Ok(content) = std::fs::read_to_string(&cmake) {
                if content.contains("project(") {
                    let has_build = path.join("build").exists();

                    // Try to read target from sdkconfig
                    let target = path
                        .join("sdkconfig")
                        .exists()
                        .then(|| {
                            std::fs::read_to_string(path.join("sdkconfig"))
                                .ok()
                                .and_then(|content| {
                                    content.lines().find_map(|line| {
                                        line.strip_prefix("CONFIG_IDF_TARGET=")
                                            .map(|v| v.trim_matches('"').to_string())
                                    })
                                })
                        })
                        .flatten();

                    let rel_path = path
                        .strip_prefix(base_dir)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();

                    projects.push(ProjectInfo {
                        name: rel_path.clone(),
                        path: path.to_string_lossy().to_string(),
                        has_build,
                        target,
                    });

                    // Don't recurse into projects
                    continue;
                }
            }
        }

        // Recurse into subdirectories (max depth managed by directory structure)
        scan_projects(&path, base_dir, projects);
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
    async fn test_list_targets() {
        let handler = EspIdfBuildToolHandler::default();
        let result = handler
            .list_targets(Parameters(ListTargetsArgs {}))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        let targets = parsed["targets"].as_array().unwrap();

        assert!(!targets.is_empty());
        let names: Vec<&str> = targets.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"esp32"));
        assert!(names.contains(&"esp32s3"));
        assert!(names.contains(&"esp32c3"));
        assert!(names.contains(&"esp32p4"));
    }

    #[tokio::test]
    async fn test_list_targets_has_arch() {
        let handler = EspIdfBuildToolHandler::default();
        let result = handler
            .list_targets(Parameters(ListTargetsArgs {}))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        let targets = parsed["targets"].as_array().unwrap();

        for target in targets {
            let arch = target["arch"].as_str().unwrap();
            assert!(
                arch == "xtensa" || arch == "riscv",
                "unexpected arch: {}",
                arch
            );
        }
    }

    #[tokio::test]
    async fn test_list_projects_empty_dir() {
        let tmp = TempDir::new().unwrap();

        let handler = EspIdfBuildToolHandler::default();
        let result = handler
            .list_projects(Parameters(ListProjectsArgs {
                projects_dir: Some(tmp.path().to_str().unwrap().to_string()),
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        assert!(parsed["projects"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_list_projects_with_projects() {
        let tmp = TempDir::new().unwrap();

        // Create a dummy ESP-IDF project
        let proj = tmp.path().join("test_project");
        fs::create_dir_all(&proj).unwrap();
        fs::write(
            proj.join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.16)\ninclude($ENV{IDF_PATH}/cmake/project.cmake)\nproject(test_project)\n",
        )
        .unwrap();

        // Create a non-project directory (no project() call)
        let non_proj = tmp.path().join("not_a_project");
        fs::create_dir_all(&non_proj).unwrap();
        fs::write(
            non_proj.join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.16)\n",
        )
        .unwrap();

        let handler = EspIdfBuildToolHandler::default();
        let result = handler
            .list_projects(Parameters(ListProjectsArgs {
                projects_dir: Some(tmp.path().to_str().unwrap().to_string()),
            }))
            .await
            .unwrap();

        let parsed = extract_json(&result);
        let projects = parsed["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["has_build"].as_bool().unwrap(), false);
    }

    #[tokio::test]
    async fn test_list_projects_no_dir() {
        let handler = EspIdfBuildToolHandler::default();
        let result = handler
            .list_projects(Parameters(ListProjectsArgs {
                projects_dir: Some("/tmp/nonexistent_dir_xyz".to_string()),
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_build_status_unknown_id() {
        let handler = EspIdfBuildToolHandler::default();
        let result = handler
            .build_status(Parameters(BuildStatusArgs {
                build_id: "nonexistent-id".to_string(),
            }))
            .await;
        assert!(result.is_err());
    }
}

#[tool_handler]
impl ServerHandler for EspIdfBuildToolHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "ESP-IDF Build MCP Server - Build, flash, and monitor ESP-IDF applications. \
                 8 tools available: list_projects, list_targets, set_target, build, flash, clean, build_status, monitor."
                    .to_string(),
            ),
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        info!("ESP-IDF Build MCP server initialized with 8 tools");
        Ok(self.get_info())
    }
}
