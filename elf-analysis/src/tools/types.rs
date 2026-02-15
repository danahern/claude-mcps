use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

// ============================================================================
// analyze_size
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalyzeSizeArgs {
    /// Path to ELF file
    pub elf_path: String,
    /// Target: "rom", "ram", or "all" (default: "all")
    #[serde(default)]
    pub target: Option<String>,
    /// Tree depth limit (default: unlimited)
    #[serde(default)]
    pub depth: Option<u32>,
    /// Override Zephyr workspace path
    #[serde(default)]
    pub workspace_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AnalyzeSizeResult {
    pub elf_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rom: Option<SizeReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ram: Option<SizeReport>,
}

// ============================================================================
// compare_sizes
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompareSizesArgs {
    /// Path to "before" ELF file
    pub elf_path_a: String,
    /// Path to "after" ELF file
    pub elf_path_b: String,
    /// Override Zephyr workspace path
    #[serde(default)]
    pub workspace_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CompareSizesResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rom: Option<SizeDelta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ram: Option<SizeDelta>,
}

// ============================================================================
// top_consumers
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TopConsumersArgs {
    /// Path to ELF file
    pub elf_path: String,
    /// Target: "rom" or "ram"
    pub target: String,
    /// Number of top consumers to return (default: 20)
    #[serde(default)]
    pub limit: Option<u32>,
    /// Grouping level: "file" (default) or "symbol"
    #[serde(default)]
    pub level: Option<String>,
    /// Override Zephyr workspace path
    #[serde(default)]
    pub workspace_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TopConsumersResult {
    pub target: String,
    pub total_size: u64,
    pub consumers: Vec<Consumer>,
}

// ============================================================================
// Shared types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizeReport {
    pub total_size: u64,
    pub used_size: u64,
    pub tree: SizeNode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizeNode {
    pub name: String,
    pub size: u64,
    #[serde(default)]
    pub children: Vec<SizeNode>,
}

#[derive(Debug, Serialize)]
pub struct SizeDelta {
    pub before: u64,
    pub after: u64,
    pub delta: i64,
    pub percent_change: f64,
    pub top_increases: Vec<NodeDelta>,
    pub top_decreases: Vec<NodeDelta>,
}

#[derive(Debug, Serialize)]
pub struct NodeDelta {
    pub path: String,
    pub before: u64,
    pub after: u64,
    pub delta: i64,
}

#[derive(Debug, Serialize)]
pub struct Consumer {
    pub path: String,
    pub size: u64,
    pub percent: f64,
}

// ============================================================================
// size_report JSON deserialization (maps from size_report output)
// ============================================================================

/// Raw JSON structure from size_report's DictExporter
#[derive(Debug, Deserialize)]
pub(crate) struct SizeReportJson {
    pub symbols: SizeReportNode,
    pub total_size: u64,
}

/// Node in the size_report JSON tree.
/// size_report uses `identifier` for the full path and `name` for display.
#[derive(Debug, Deserialize)]
pub(crate) struct SizeReportNode {
    pub identifier: String,
    #[allow(dead_code)]
    pub name: String,
    pub size: u64,
    #[serde(default)]
    pub children: Vec<SizeReportNode>,
}
