use serde::{Deserialize, Serialize};
use std::path::Path;

/// A structured knowledge item stored as YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeItem {
    pub id: String,
    pub title: String,
    pub body: String,

    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default = "default_severity")]
    pub severity: String,

    #[serde(default)]
    pub applies_to: AppliesTo,

    #[serde(default)]
    pub file_patterns: Vec<String>,

    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub validated_by: Vec<String>,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub superseded_by: Option<String>,

    pub created: String,
    #[serde(default)]
    pub updated: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub source_session: Option<String>,

    /// Legacy tags from migration (optional, for search compatibility)
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppliesTo {
    #[serde(default)]
    pub boards: Vec<String>,
    #[serde(default)]
    pub chips: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub subsystems: Vec<String>,
}

fn default_category() -> String { "operational".to_string() }
fn default_severity() -> String { "informational".to_string() }
fn default_status() -> String { "unvalidated".to_string() }

impl KnowledgeItem {
    /// Generate a new ID based on date and sequence number.
    pub fn generate_id(date: &str, sequence: u32) -> String {
        // date format: 2026-02-14 → k-2026-0214-001
        let compact_date = date.replace('-', "");
        // Take YYYYMMDD -> drop first 0 from month if needed, actually keep full
        // Format: k-YYYY-MMDD-NNN
        if compact_date.len() == 8 {
            format!("k-{}-{}-{:03}", &compact_date[..4], &compact_date[4..], sequence)
        } else {
            format!("k-{}-{:03}", compact_date, sequence)
        }
    }

    /// Load a knowledge item from a YAML file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        serde_yaml::from_str(&contents)
            .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
    }

    /// Save a knowledge item to a YAML file.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }
        let yaml = serde_yaml::to_string(self)
            .map_err(|e| format!("Failed to serialize: {}", e))?;
        std::fs::write(path, yaml)
            .map_err(|e| format!("Failed to write {}: {}", path.display(), e))
    }

    /// All searchable text combined for FTS indexing.
    pub fn searchable_text(&self) -> String {
        let mut parts = vec![
            self.title.clone(),
            self.body.clone(),
            self.category.clone(),
        ];
        parts.extend(self.tags.iter().cloned());
        parts.extend(self.applies_to.boards.iter().cloned());
        parts.extend(self.applies_to.chips.iter().cloned());
        parts.extend(self.applies_to.tools.iter().cloned());
        parts.extend(self.applies_to.subsystems.iter().cloned());
        parts.join(" ")
    }
}

/// Load all knowledge items from a directory.
pub fn load_all_items(items_dir: &Path) -> Result<Vec<KnowledgeItem>, String> {
    let mut items = Vec::new();
    if !items_dir.exists() {
        return Ok(items);
    }

    let pattern = items_dir.join("*.yml");
    let pattern_str = pattern.to_string_lossy();
    let entries = glob::glob(&pattern_str)
        .map_err(|e| format!("Invalid glob pattern: {}", e))?;

    for entry in entries {
        let path = entry.map_err(|e| format!("Glob error: {}", e))?;
        match KnowledgeItem::load(&path) {
            Ok(item) => items.push(item),
            Err(e) => tracing::warn!("Skipping {}: {}", path.display(), e),
        }
    }

    items.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(items)
}
