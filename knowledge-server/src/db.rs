use rusqlite::{Connection, params};
use std::path::Path;
use tracing::info;

use crate::knowledge::KnowledgeItem;

/// SQLite database with FTS5 index for knowledge items.
pub struct KnowledgeDb {
    conn: Connection,
}

impl KnowledgeDb {
    /// Open or create the database at the given path.
    pub fn open(db_path: &Path) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create cache dir: {}", e))?;
        }
        let conn = Connection::open(db_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;

        let db = Self { conn };
        db.create_tables()?;
        Ok(db)
    }

    /// Open an in-memory database (for testing).
    pub fn open_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to open in-memory database: {}", e))?;
        let db = Self { conn };
        db.create_tables()?;
        Ok(db)
    }

    fn create_tables(&self) -> Result<(), String> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS items (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                body TEXT NOT NULL,
                category TEXT NOT NULL,
                severity TEXT NOT NULL,
                status TEXT NOT NULL,
                deprecated INTEGER NOT NULL DEFAULT 0,
                superseded_by TEXT,
                created TEXT NOT NULL,
                updated TEXT NOT NULL,
                author TEXT NOT NULL,
                file_hash TEXT NOT NULL,
                -- Denormalized for fast queries
                boards_json TEXT NOT NULL DEFAULT '[]',
                chips_json TEXT NOT NULL DEFAULT '[]',
                tools_json TEXT NOT NULL DEFAULT '[]',
                subsystems_json TEXT NOT NULL DEFAULT '[]',
                tags_json TEXT NOT NULL DEFAULT '[]',
                file_patterns_json TEXT NOT NULL DEFAULT '[]'
            );

            CREATE TABLE IF NOT EXISTS items_fts (
                id TEXT,
                searchable_text TEXT
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS items_fts5 USING fts5(
                id,
                searchable_text,
                content='items_fts',
                content_rowid='rowid'
            );

            -- Triggers to keep FTS5 in sync
            CREATE TRIGGER IF NOT EXISTS items_fts_ai AFTER INSERT ON items_fts BEGIN
                INSERT INTO items_fts5(rowid, id, searchable_text) VALUES (new.rowid, new.id, new.searchable_text);
            END;
            CREATE TRIGGER IF NOT EXISTS items_fts_ad AFTER DELETE ON items_fts BEGIN
                INSERT INTO items_fts5(items_fts5, rowid, id, searchable_text) VALUES ('delete', old.rowid, old.id, old.searchable_text);
            END;

            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );"
        ).map_err(|e| format!("Failed to create tables: {}", e))?;
        Ok(())
    }

    /// Index a single knowledge item. Upserts by id.
    pub fn index_item(&self, item: &KnowledgeItem, file_hash: &str) -> Result<(), String> {
        // Delete existing entry if any
        self.conn.execute("DELETE FROM items_fts WHERE id = ?1", params![item.id])
            .map_err(|e| format!("Failed to delete FTS entry: {}", e))?;
        self.conn.execute("DELETE FROM items WHERE id = ?1", params![item.id])
            .map_err(|e| format!("Failed to delete item: {}", e))?;

        // Insert item
        self.conn.execute(
            "INSERT INTO items (id, title, body, category, severity, status, deprecated, superseded_by,
                created, updated, author, file_hash, boards_json, chips_json, tools_json,
                subsystems_json, tags_json, file_patterns_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                item.id, item.title, item.body, item.category, item.severity,
                item.status, item.deprecated as i32, item.superseded_by,
                item.created, item.updated, item.author, file_hash,
                serde_json::to_string(&item.applies_to.boards).unwrap_or_default(),
                serde_json::to_string(&item.applies_to.chips).unwrap_or_default(),
                serde_json::to_string(&item.applies_to.tools).unwrap_or_default(),
                serde_json::to_string(&item.applies_to.subsystems).unwrap_or_default(),
                serde_json::to_string(&item.tags).unwrap_or_default(),
                serde_json::to_string(&item.file_patterns).unwrap_or_default(),
            ],
        ).map_err(|e| format!("Failed to insert item: {}", e))?;

        // Insert FTS entry
        self.conn.execute(
            "INSERT INTO items_fts (id, searchable_text) VALUES (?1, ?2)",
            params![item.id, item.searchable_text()],
        ).map_err(|e| format!("Failed to insert FTS entry: {}", e))?;

        Ok(())
    }

    /// Full-text search across knowledge items. Returns matching item IDs ranked by relevance.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<String>, String> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM items_fts5 WHERE searchable_text MATCH ?1
             ORDER BY rank LIMIT ?2"
        ).map_err(|e| format!("Failed to prepare search: {}", e))?;

        let ids: Vec<String> = stmt.query_map(params![query, limit as i64], |row| {
            row.get(0)
        })
        .map_err(|e| format!("Search failed: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(ids)
    }

    /// Find items matching a board (or its chip/family/arch via provided values).
    pub fn items_for_board(&self, board: &str, chip: &str, family: &str, arch: &str) -> Result<Vec<String>, String> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM items WHERE
                (boards_json LIKE ?1 OR chips_json LIKE ?2 OR
                 subsystems_json LIKE ?3 OR subsystems_json LIKE ?4)
                AND deprecated = 0
             ORDER BY
                CASE severity WHEN 'critical' THEN 0 WHEN 'important' THEN 1 ELSE 2 END,
                created DESC"
        ).map_err(|e| format!("Failed to prepare board query: {}", e))?;

        let board_pattern = format!("%\"{}\"%" , board);
        let chip_pattern = format!("%\"{}\"%" , chip);
        let family_pattern = format!("%\"{}\"%" , family);
        let arch_pattern = format!("%\"{}\"%" , arch);

        let ids: Vec<String> = stmt.query_map(
            params![board_pattern, chip_pattern, family_pattern, arch_pattern],
            |row| row.get(0),
        )
        .map_err(|e| format!("Board query failed: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(ids)
    }

    /// Find items whose file_patterns match any of the given file paths.
    pub fn items_for_files(&self, files: &[String]) -> Result<Vec<String>, String> {
        // Get all non-deprecated items with file patterns
        let mut stmt = self.conn.prepare(
            "SELECT id, file_patterns_json FROM items WHERE deprecated = 0 AND file_patterns_json != '[]'"
        ).map_err(|e| format!("Failed to prepare file query: {}", e))?;

        let rows: Vec<(String, String)> = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("File query failed: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        let mut matching_ids = Vec::new();
        for (id, patterns_json) in &rows {
            let patterns: Vec<String> = serde_json::from_str(patterns_json).unwrap_or_default();
            for pattern in &patterns {
                let glob_pattern = glob::Pattern::new(pattern).ok();
                if let Some(gp) = glob_pattern {
                    for file in files {
                        if gp.matches(file) {
                            matching_ids.push(id.clone());
                            break;
                        }
                    }
                }
            }
        }

        matching_ids.dedup();
        Ok(matching_ids)
    }

    /// Get items created or updated in the last N days.
    pub fn recent_items(&self, days: u32) -> Result<Vec<String>, String> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
        let cutoff_str = cutoff.format("%Y-%m-%d").to_string();

        let mut stmt = self.conn.prepare(
            "SELECT id FROM items WHERE updated >= ?1 OR created >= ?1
             ORDER BY updated DESC, created DESC"
        ).map_err(|e| format!("Failed to prepare recent query: {}", e))?;

        let ids: Vec<String> = stmt.query_map(params![cutoff_str], |row| row.get(0))
            .map_err(|e| format!("Recent query failed: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(ids)
    }

    /// Get items not updated in the last N days (potentially stale).
    pub fn stale_items(&self, days: u32) -> Result<Vec<String>, String> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
        let cutoff_str = cutoff.format("%Y-%m-%d").to_string();

        let mut stmt = self.conn.prepare(
            "SELECT id FROM items WHERE updated < ?1 AND deprecated = 0
             ORDER BY updated ASC"
        ).map_err(|e| format!("Failed to prepare stale query: {}", e))?;

        let ids: Vec<String> = stmt.query_map(params![cutoff_str], |row| row.get(0))
            .map_err(|e| format!("Stale query failed: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(ids)
    }

    /// Get all unique tags across all items.
    pub fn all_tags(&self) -> Result<Vec<String>, String> {
        let mut stmt = self.conn.prepare(
            "SELECT tags_json FROM items WHERE deprecated = 0"
        ).map_err(|e| format!("Failed to prepare tags query: {}", e))?;

        let mut all_tags = std::collections::BTreeSet::new();
        let rows: Vec<String> = stmt.query_map([], |row| row.get(0))
            .map_err(|e| format!("Tags query failed: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        for tags_json in &rows {
            let tags: Vec<String> = serde_json::from_str(tags_json).unwrap_or_default();
            all_tags.extend(tags);
        }

        // Also collect boards, chips, tools, subsystems as tags
        let mut stmt2 = self.conn.prepare(
            "SELECT boards_json, chips_json, tools_json, subsystems_json FROM items WHERE deprecated = 0"
        ).map_err(|e| format!("Failed to prepare scope query: {}", e))?;

        let scope_rows: Vec<(String, String, String, String)> = stmt2.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| format!("Scope query failed: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        for (boards, chips, tools, subsystems) in &scope_rows {
            let b: Vec<String> = serde_json::from_str(boards).unwrap_or_default();
            let c: Vec<String> = serde_json::from_str(chips).unwrap_or_default();
            let t: Vec<String> = serde_json::from_str(tools).unwrap_or_default();
            let s: Vec<String> = serde_json::from_str(subsystems).unwrap_or_default();
            all_tags.extend(b);
            all_tags.extend(c);
            all_tags.extend(t);
            all_tags.extend(s);
        }

        Ok(all_tags.into_iter().collect())
    }

    /// Get item by ID (returns raw row data as JSON string for the handler to deserialize).
    pub fn get_item_raw(&self, id: &str) -> Result<Option<ItemRow>, String> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, body, category, severity, status, deprecated, superseded_by,
                    created, updated, author, boards_json, chips_json, tools_json,
                    subsystems_json, tags_json, file_patterns_json
             FROM items WHERE id = ?1"
        ).map_err(|e| format!("Failed to prepare get query: {}", e))?;

        let result = stmt.query_row(params![id], |row| {
            Ok(ItemRow {
                id: row.get(0)?,
                title: row.get(1)?,
                body: row.get(2)?,
                category: row.get(3)?,
                severity: row.get(4)?,
                status: row.get(5)?,
                deprecated: row.get::<_, i32>(6)? != 0,
                superseded_by: row.get(7)?,
                created: row.get(8)?,
                updated: row.get(9)?,
                author: row.get(10)?,
                boards_json: row.get(11)?,
                chips_json: row.get(12)?,
                tools_json: row.get(13)?,
                subsystems_json: row.get(14)?,
                tags_json: row.get(15)?,
                file_patterns_json: row.get(16)?,
            })
        });

        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Get query failed: {}", e)),
        }
    }

    /// Get the stored file hash for an item.
    pub fn get_file_hash(&self, id: &str) -> Result<Option<String>, String> {
        let mut stmt = self.conn.prepare("SELECT file_hash FROM items WHERE id = ?1")
            .map_err(|e| format!("Failed to prepare hash query: {}", e))?;

        let result = stmt.query_row(params![id], |row| row.get::<_, String>(0));
        match result {
            Ok(hash) => Ok(Some(hash)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Hash query failed: {}", e)),
        }
    }

    /// Get all indexed item IDs.
    pub fn all_item_ids(&self) -> Result<Vec<String>, String> {
        let mut stmt = self.conn.prepare("SELECT id FROM items ORDER BY id")
            .map_err(|e| format!("Failed to prepare IDs query: {}", e))?;
        let ids: Vec<String> = stmt.query_map([], |row| row.get(0))
            .map_err(|e| format!("IDs query failed: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(ids)
    }

    /// Delete an item from the index.
    pub fn delete_item(&self, id: &str) -> Result<(), String> {
        self.conn.execute("DELETE FROM items_fts WHERE id = ?1", params![id])
            .map_err(|e| format!("Failed to delete FTS entry: {}", e))?;
        self.conn.execute("DELETE FROM items WHERE id = ?1", params![id])
            .map_err(|e| format!("Failed to delete item: {}", e))?;
        Ok(())
    }

    /// Rebuild the entire index from a set of items.
    pub fn rebuild(&self, items: &[KnowledgeItem]) -> Result<(), String> {
        info!("Rebuilding index with {} items", items.len());

        self.conn.execute("DELETE FROM items_fts", [])
            .map_err(|e| format!("Failed to clear FTS: {}", e))?;
        self.conn.execute("DELETE FROM items", [])
            .map_err(|e| format!("Failed to clear items: {}", e))?;

        for item in items {
            let hash = compute_item_hash(item);
            self.index_item(item, &hash)?;
        }

        info!("Index rebuilt with {} items", items.len());
        Ok(())
    }

    /// Get items filtered by category.
    pub fn items_by_category(&self, category: &str) -> Result<Vec<String>, String> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM items WHERE category = ?1 AND deprecated = 0 ORDER BY created DESC"
        ).map_err(|e| format!("Failed to prepare category query: {}", e))?;

        let ids: Vec<String> = stmt.query_map(params![category], |row| row.get(0))
            .map_err(|e| format!("Category query failed: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(ids)
    }

    /// Get items filtered by severity.
    pub fn items_by_severity(&self, severity: &str) -> Result<Vec<String>, String> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM items WHERE severity = ?1 AND deprecated = 0 ORDER BY created DESC"
        ).map_err(|e| format!("Failed to prepare severity query: {}", e))?;

        let ids: Vec<String> = stmt.query_map(params![severity], |row| row.get(0))
            .map_err(|e| format!("Severity query failed: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(ids)
    }
}

/// Raw database row for an item.
#[derive(Debug, Clone)]
pub struct ItemRow {
    pub id: String,
    pub title: String,
    pub body: String,
    pub category: String,
    pub severity: String,
    pub status: String,
    pub deprecated: bool,
    pub superseded_by: Option<String>,
    pub created: String,
    pub updated: String,
    pub author: String,
    pub boards_json: String,
    pub chips_json: String,
    pub tools_json: String,
    pub subsystems_json: String,
    pub tags_json: String,
    pub file_patterns_json: String,
}

impl ItemRow {
    /// Convert to a summary for search results.
    pub fn to_summary(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "title": self.title,
            "body": self.body,
            "category": self.category,
            "severity": self.severity,
            "status": self.status,
            "created": self.created,
            "updated": self.updated,
            "author": self.author,
            "boards": serde_json::from_str::<Vec<String>>(&self.boards_json).unwrap_or_default(),
            "chips": serde_json::from_str::<Vec<String>>(&self.chips_json).unwrap_or_default(),
            "tools": serde_json::from_str::<Vec<String>>(&self.tools_json).unwrap_or_default(),
            "tags": serde_json::from_str::<Vec<String>>(&self.tags_json).unwrap_or_default(),
        })
    }
}

/// Compute a hash of a knowledge item's content for change detection.
pub fn compute_item_hash(item: &KnowledgeItem) -> String {
    use sha2::{Sha256, Digest};
    let content = serde_yaml::to_string(item).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

/// Compute a hash of a file's contents.
pub fn compute_file_hash(path: &Path) -> Result<String, String> {
    use sha2::{Sha256, Digest};
    let content = std::fs::read(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    Ok(hex::encode(hasher.finalize()))
}
