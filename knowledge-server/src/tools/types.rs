use serde::Deserialize;
use schemars::JsonSchema;

// ============================================================================
// Knowledge Store tool args
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CaptureArgs {
    /// Title of the knowledge item
    pub title: String,

    /// Body text (the actual learning content)
    pub body: String,

    /// Category: hardware, toolchain, pattern, operational
    #[serde(default)]
    pub category: Option<String>,

    /// Severity: critical, important, informational
    #[serde(default)]
    pub severity: Option<String>,

    /// Board names this applies to (e.g., ["nrf54l15dk"])
    #[serde(default)]
    pub boards: Option<Vec<String>>,

    /// Chip names this applies to (e.g., ["nrf54l15"])
    #[serde(default)]
    pub chips: Option<Vec<String>>,

    /// Tool names this applies to (e.g., ["probe-rs"])
    #[serde(default)]
    pub tools: Option<Vec<String>>,

    /// Subsystem names (e.g., ["coredump", "logging"])
    #[serde(default)]
    pub subsystems: Option<Vec<String>>,

    /// File glob patterns that should trigger this knowledge
    #[serde(default)]
    pub file_patterns: Option<Vec<String>>,

    /// Legacy tags for search compatibility
    #[serde(default)]
    pub tags: Option<Vec<String>>,

    /// Author name
    #[serde(default)]
    pub author: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchArgs {
    /// Search query (supports FTS5 syntax: AND, OR, NOT, phrases)
    pub query: String,

    /// Filter by tags
    #[serde(default)]
    pub tags: Option<Vec<String>>,

    /// Filter by chips
    #[serde(default)]
    pub chips: Option<Vec<String>>,

    /// Filter by category
    #[serde(default)]
    pub category: Option<String>,

    /// Maximum results to return
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ForContextArgs {
    /// File paths currently being worked on
    pub files: Vec<String>,

    /// Board being targeted (for hardware-aware retrieval)
    #[serde(default)]
    pub board: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeprecateArgs {
    /// Knowledge item ID to deprecate
    pub id: String,

    /// ID of the item that supersedes this one
    #[serde(default)]
    pub superseded_by: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ValidateArgs {
    /// Knowledge item ID to validate
    pub id: String,

    /// Name of the person validating
    pub validated_by: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecentArgs {
    /// Number of days to look back (default: 7)
    #[serde(default)]
    pub days: Option<u32>,

    /// Maximum results to return (default: 20)
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StaleArgs {
    /// Items not updated in this many days are considered stale (default: 90)
    #[serde(default)]
    pub days: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListTagsArgs {
    /// Filter tags by prefix
    #[serde(default)]
    pub prefix: Option<String>,
}

// ============================================================================
// Board Profile tool args
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BoardInfoArgs {
    /// Board name (e.g., "nrf54l15dk")
    pub board: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ForChipArgs {
    /// Chip name (e.g., "nrf54l15")
    pub chip: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ForBoardArgs {
    /// Board name (e.g., "nrf54l15dk")
    pub board: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListBoardsArgs {
    /// Filter by vendor
    #[serde(default)]
    pub vendor: Option<String>,
}

// ============================================================================
// Auto-generation tool args
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RegenerateRulesArgs {
    /// Dry run: show what would be generated without writing files
    #[serde(default)]
    pub dry_run: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RegenerateGotchasArgs {
    /// Dry run: show what would be generated without writing files
    #[serde(default)]
    pub dry_run: Option<bool>,
}
