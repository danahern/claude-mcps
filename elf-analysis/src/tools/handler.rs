use rmcp::{
    tool, tool_router, tool_handler, ServerHandler,
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::*,
    ErrorData as McpError,
};
use tracing::info;
use std::future::Future;
use std::path::{Path, PathBuf};

use super::types::*;
use super::size_report;
use crate::config::Config;

#[derive(Clone)]
pub struct ElfAnalysisToolHandler {
    #[allow(dead_code)]
    tool_router: ToolRouter<ElfAnalysisToolHandler>,
    config: Config,
}

impl ElfAnalysisToolHandler {
    pub fn new(config: Config) -> Self {
        Self {
            tool_router: Self::tool_router(),
            config,
        }
    }

    /// Resolve the zephyr_base path from per-call override or config.
    fn resolve_zephyr_base(&self, workspace_override: Option<&str>) -> Result<PathBuf, McpError> {
        // Per-call workspace override
        if let Some(ws) = workspace_override {
            let zb = PathBuf::from(ws).join("zephyr");
            if zb.exists() {
                return Ok(zb);
            }
        }

        // Config zephyr_base
        if let Some(zb) = &self.config.zephyr_base {
            if zb.exists() {
                return Ok(zb.clone());
            }
        }

        Err(McpError::internal_error(
            "Zephyr base not found. Set --workspace or --zephyr-base when starting the server, \
             or pass workspace_path per-call.".to_string(),
            None,
        ))
    }

    /// Resolve workspace path from per-call override or config.
    fn resolve_workspace(&self, workspace_override: Option<&str>) -> Option<PathBuf> {
        workspace_override
            .map(PathBuf::from)
            .or_else(|| self.config.workspace_path.clone())
    }

    /// Validate that an ELF file exists.
    fn validate_elf(path: &str) -> Result<PathBuf, McpError> {
        let p = PathBuf::from(path);
        if !p.exists() {
            return Err(McpError::invalid_params(
                format!("ELF file not found: {}", path),
                None,
            ));
        }
        Ok(p)
    }

    /// Run size_report and parse results for the given targets.
    async fn run_and_parse(
        &self,
        elf_path: &Path,
        workspace_override: Option<&str>,
        targets: &[&str],
    ) -> Result<(Option<SizeReport>, Option<SizeReport>), McpError> {
        let zephyr_base = self.resolve_zephyr_base(workspace_override)?;
        let workspace = self.resolve_workspace(workspace_override);

        let json_files = size_report::run_size_report(
            elf_path,
            &zephyr_base,
            workspace.as_deref(),
            targets,
        ).await.map_err(|e| McpError::internal_error(e, None))?;

        let rom = if let Some(path) = json_files.get("rom") {
            Some(size_report::parse_size_json(path)
                .map_err(|e| McpError::internal_error(e, None))?)
        } else {
            None
        };

        let ram = if let Some(path) = json_files.get("ram") {
            Some(size_report::parse_size_json(path)
                .map_err(|e| McpError::internal_error(e, None))?)
        } else {
            None
        };

        Ok((rom, ram))
    }
}

#[tool_router]
impl ElfAnalysisToolHandler {
    #[tool(description = "Full ROM/RAM breakdown with per-file/module attribution. Answers 'how much flash/RAM am I using?' and 'where is memory going?'")]
    async fn analyze_size(&self, Parameters(args): Parameters<AnalyzeSizeArgs>) -> Result<CallToolResult, McpError> {
        let elf_path = Self::validate_elf(&args.elf_path)?;
        let target = args.target.as_deref().unwrap_or("all");

        let targets: Vec<&str> = match target {
            "rom" => vec!["rom"],
            "ram" => vec!["ram"],
            _ => vec!["all"],
        };

        info!("analyze_size: {} target={}", elf_path.display(), target);

        let (rom, ram) = self.run_and_parse(&elf_path, args.workspace_path.as_deref(), &targets).await?;

        // Apply depth truncation if requested
        let rom = rom.map(|r| {
            if let Some(depth) = args.depth {
                SizeReport {
                    total_size: r.total_size,
                    used_size: r.used_size,
                    tree: size_report::truncate_tree(&r.tree, depth),
                }
            } else {
                r
            }
        });

        let ram = ram.map(|r| {
            if let Some(depth) = args.depth {
                SizeReport {
                    total_size: r.total_size,
                    used_size: r.used_size,
                    tree: size_report::truncate_tree(&r.tree, depth),
                }
            } else {
                r
            }
        });

        let result = AnalyzeSizeResult {
            elf_path: args.elf_path,
            rom,
            ram,
        };

        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(format!("Serialization error: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Diff two ELFs to track size growth. Answers 'did this change bloat the binary?' and 'what grew?'")]
    async fn compare_sizes(&self, Parameters(args): Parameters<CompareSizesArgs>) -> Result<CallToolResult, McpError> {
        let elf_a = Self::validate_elf(&args.elf_path_a)?;
        let elf_b = Self::validate_elf(&args.elf_path_b)?;

        info!("compare_sizes: {} vs {}", elf_a.display(), elf_b.display());

        let ws = args.workspace_path.as_deref();
        let (rom_a, ram_a) = self.run_and_parse(&elf_a, ws, &["all"]).await?;
        let (rom_b, ram_b) = self.run_and_parse(&elf_b, ws, &["all"]).await?;

        let rom_delta = match (rom_a, rom_b) {
            (Some(a), Some(b)) => {
                let (increases, decreases) = size_report::diff_trees(&a, &b, 10);
                let delta = b.used_size as i64 - a.used_size as i64;
                let percent = if a.used_size > 0 {
                    (delta as f64 / a.used_size as f64) * 100.0
                } else {
                    0.0
                };
                Some(SizeDelta {
                    before: a.used_size,
                    after: b.used_size,
                    delta,
                    percent_change: percent,
                    top_increases: increases,
                    top_decreases: decreases,
                })
            }
            _ => None,
        };

        let ram_delta = match (ram_a, ram_b) {
            (Some(a), Some(b)) => {
                let (increases, decreases) = size_report::diff_trees(&a, &b, 10);
                let delta = b.used_size as i64 - a.used_size as i64;
                let percent = if a.used_size > 0 {
                    (delta as f64 / a.used_size as f64) * 100.0
                } else {
                    0.0
                };
                Some(SizeDelta {
                    before: a.used_size,
                    after: b.used_size,
                    delta,
                    percent_change: percent,
                    top_increases: increases,
                    top_decreases: decreases,
                })
            }
            _ => None,
        };

        let result = CompareSizesResult {
            rom: rom_delta,
            ram: ram_delta,
        };

        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(format!("Serialization error: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Quick 'show me the biggest files/symbols' view. Flattens the tree and sorts by size.")]
    async fn top_consumers(&self, Parameters(args): Parameters<TopConsumersArgs>) -> Result<CallToolResult, McpError> {
        let elf_path = Self::validate_elf(&args.elf_path)?;
        let target = &args.target;
        let limit = args.limit.unwrap_or(20) as usize;
        let level = args.level.as_deref().unwrap_or("file");

        if target != "rom" && target != "ram" {
            return Err(McpError::invalid_params(
                "target must be \"rom\" or \"ram\"".to_string(),
                None,
            ));
        }

        info!("top_consumers: {} target={} level={}", elf_path.display(), target, level);

        let targets = vec![target.as_str()];
        let (rom, ram) = self.run_and_parse(&elf_path, args.workspace_path.as_deref(), &targets).await?;

        let report = match target.as_str() {
            "rom" => rom.ok_or_else(|| McpError::internal_error("No ROM report generated".to_string(), None))?,
            "ram" => ram.ok_or_else(|| McpError::internal_error("No RAM report generated".to_string(), None))?,
            _ => unreachable!(),
        };

        let mut consumers = size_report::flatten_tree(&report.tree, level);
        consumers.truncate(limit);

        let result = TopConsumersResult {
            target: target.clone(),
            total_size: report.total_size,
            consumers,
        };

        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(format!("Serialization error: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

#[tool_handler]
impl ServerHandler for ElfAnalysisToolHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "ELF Analysis MCP Server - Analyze ROM/RAM usage in ELF binaries. \
                 3 tools available: analyze_size, compare_sizes, top_consumers.".to_string()
            ),
        }
    }
}
