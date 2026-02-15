use serde::{Deserialize, Serialize};
use std::path::Path;

/// A board profile defining hardware hierarchy and capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardProfile {
    pub board: String,
    pub chip: String,
    pub family: String,
    pub arch: String,
    pub vendor: String,

    // Build & flash config
    pub flash_method: String,
    pub flash_tool: String,
    #[serde(default)]
    pub probe: Option<String>,
    #[serde(default)]
    pub connect_under_reset: bool,
    #[serde(default)]
    pub target_chip: Option<String>,
    #[serde(default)]
    pub board_qualifier: Option<String>,

    // Memory
    #[serde(default)]
    pub memory: MemoryConfig,

    // Capabilities
    #[serde(default)]
    pub peripherals: Vec<String>,
    #[serde(default)]
    pub features: Vec<String>,

    // Errata
    #[serde(default)]
    pub errata_doc: Option<String>,
    #[serde(default)]
    pub known_errata: Vec<ErrataEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryConfig {
    #[serde(default)]
    pub flash: Option<FlashConfig>,
    #[serde(default)]
    pub ram: Option<RamConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashConfig {
    #[serde(rename = "type")]
    pub flash_type: String,
    pub size_kb: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RamConfig {
    pub size_kb: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrataEntry {
    pub id: String,
    pub summary: String,
    #[serde(default)]
    pub workaround: Option<String>,
    #[serde(default)]
    pub knowledge_items: Vec<String>,
}

impl BoardProfile {
    /// Load a board profile from a YAML file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        serde_yaml::from_str(&contents)
            .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
    }

    /// Get all hierarchy levels for this board (board, chip, family, arch).
    pub fn hierarchy(&self) -> Vec<(&str, &str)> {
        vec![
            ("board", &self.board),
            ("chip", &self.chip),
            ("family", &self.family),
            ("arch", &self.arch),
        ]
    }
}

/// Load all board profiles from a directory.
pub fn load_all_boards(boards_dir: &Path) -> Result<Vec<BoardProfile>, String> {
    let mut boards = Vec::new();
    if !boards_dir.exists() {
        return Ok(boards);
    }

    let pattern = boards_dir.join("*.yml");
    let pattern_str = pattern.to_string_lossy();
    let entries = glob::glob(&pattern_str)
        .map_err(|e| format!("Invalid glob pattern: {}", e))?;

    for entry in entries {
        let path = entry.map_err(|e| format!("Glob error: {}", e))?;
        match BoardProfile::load(&path) {
            Ok(board) => boards.push(board),
            Err(e) => tracing::warn!("Skipping {}: {}", path.display(), e),
        }
    }

    boards.sort_by(|a, b| a.board.cmp(&b.board));
    Ok(boards)
}
