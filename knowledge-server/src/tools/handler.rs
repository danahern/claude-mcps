use rmcp::{
    tool, tool_router, tool_handler, ServerHandler,
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::*,
    ErrorData as McpError,
};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::info;

use super::types::*;
use crate::boards::{self, BoardProfile};
use crate::config::Config;
use crate::db::{self, KnowledgeDb};
use crate::knowledge::{self, AppliesTo, KnowledgeItem};

#[derive(Clone)]
pub struct KnowledgeToolHandler {
    #[allow(dead_code)]
    tool_router: ToolRouter<KnowledgeToolHandler>,
    config: Config,
    db: Arc<Mutex<KnowledgeDb>>,
    boards: Arc<RwLock<Vec<BoardProfile>>>,
    items: Arc<RwLock<HashMap<String, KnowledgeItem>>>,
}

impl KnowledgeToolHandler {
    pub fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        // Open or create database
        let db_path = config.cache_dir().join("index.db");
        let db = KnowledgeDb::open(&db_path)?;

        // Load board profiles
        let board_profiles = boards::load_all_boards(&config.boards_dir()).unwrap_or_default();
        info!("Loaded {} board profiles", board_profiles.len());

        // Load and index knowledge items
        let items_list = knowledge::load_all_items(&config.items_dir()).unwrap_or_default();
        info!("Loaded {} knowledge items", items_list.len());

        // Build items map and index
        let mut items_map = HashMap::new();
        for item in &items_list {
            items_map.insert(item.id.clone(), item.clone());
        }

        // Rebuild index (fast for small collections, ~2-5s at 5000 items)
        db.rebuild(&items_list)?;

        Ok(Self {
            tool_router: Self::tool_router(),
            config,
            db: Arc::new(Mutex::new(db)),
            boards: Arc::new(RwLock::new(board_profiles)),
            items: Arc::new(RwLock::new(items_map)),
        })
    }

    /// Find a board profile by name.
    async fn find_board(&self, board_name: &str) -> Option<BoardProfile> {
        let boards = self.boards.read().await;
        boards.iter().find(|b| b.board == board_name).cloned()
    }

    /// Get item details by ID, returning JSON summary.
    async fn get_item_summaries(&self, ids: &[String]) -> Vec<serde_json::Value> {
        let items = self.items.read().await;
        ids.iter()
            .filter_map(|id| {
                items.get(id).map(|item| {
                    serde_json::json!({
                        "id": item.id,
                        "title": item.title,
                        "body": item.body,
                        "category": item.category,
                        "severity": item.severity,
                        "status": item.status,
                        "created": item.created,
                        "updated": item.updated,
                        "author": item.author,
                        "applies_to": {
                            "boards": item.applies_to.boards,
                            "chips": item.applies_to.chips,
                            "tools": item.applies_to.tools,
                            "subsystems": item.applies_to.subsystems,
                        },
                        "tags": item.tags,
                        "file_patterns": item.file_patterns,
                    })
                })
            })
            .collect()
    }

    /// Get the next sequence number for a given date.
    async fn next_sequence(&self, date: &str) -> u32 {
        let items = self.items.read().await;
        // generate_id produces "k-YYYY-MMDD-NNN", so build a matching prefix
        let compact = date.replace('-', "");
        let prefix = if compact.len() == 8 {
            format!("k-{}-{}-", &compact[..4], &compact[4..])
        } else {
            format!("k-{}-", compact)
        };
        let max_seq = items.keys()
            .filter(|id| id.starts_with(&prefix))
            .filter_map(|id| {
                id.rsplit('-').next()
                    .and_then(|s| s.parse::<u32>().ok())
            })
            .max()
            .unwrap_or(0);
        max_seq + 1
    }
}

#[tool_router]
impl KnowledgeToolHandler {
    // ========================================================================
    // Knowledge Store tools
    // ========================================================================

    #[tool(description = "Capture a new knowledge item. Creates a structured YAML file and indexes it for search. Returns the created item ID and file path.")]
    async fn capture(&self, Parameters(args): Parameters<CaptureArgs>) -> Result<CallToolResult, McpError> {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let seq = self.next_sequence(&today).await;
        let id = KnowledgeItem::generate_id(&today, seq);

        let item = KnowledgeItem {
            id: id.clone(),
            title: args.title,
            body: args.body,
            category: args.category.unwrap_or_else(|| "operational".to_string()),
            severity: args.severity.unwrap_or_else(|| "informational".to_string()),
            applies_to: AppliesTo {
                boards: args.boards.unwrap_or_default(),
                chips: args.chips.unwrap_or_default(),
                tools: args.tools.unwrap_or_default(),
                subsystems: args.subsystems.unwrap_or_default(),
            },
            file_patterns: args.file_patterns.unwrap_or_default(),
            status: "unvalidated".to_string(),
            validated_by: vec![],
            deprecated: false,
            superseded_by: None,
            created: today.clone(),
            updated: today,
            author: args.author.unwrap_or_else(|| "claude".to_string()),
            source_session: None,
            tags: args.tags.unwrap_or_default(),
        };

        // Save YAML file
        let file_path = self.config.items_dir().join(format!("{}.yml", id));
        item.save(&file_path)
            .map_err(|e| McpError::internal_error(format!("Failed to save: {}", e), None))?;

        // Index in database
        let hash = db::compute_item_hash(&item);
        {
            let db = self.db.lock().await;
            db.index_item(&item, &hash)
                .map_err(|e| McpError::internal_error(format!("Failed to index: {}", e), None))?;
        }

        // Update in-memory map
        {
            let mut items = self.items.write().await;
            items.insert(id.clone(), item);
        }

        info!("Captured knowledge item: {}", id);
        let result = serde_json::json!({
            "id": id,
            "file_path": file_path.display().to_string(),
            "message": "Knowledge item created and indexed"
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result)
                .map_err(|e| McpError::internal_error(format!("JSON error: {}", e), None))?
        )]))
    }

    #[tool(description = "Search knowledge items using full-text search. Supports FTS5 query syntax (AND, OR, NOT, phrases in quotes). Returns matching items ranked by relevance.")]
    async fn search(&self, Parameters(args): Parameters<SearchArgs>) -> Result<CallToolResult, McpError> {
        let limit = args.limit.unwrap_or(20) as usize;

        let ids = {
            let db = self.db.lock().await;
            db.search(&args.query, limit * 2)  // over-fetch for post-filtering
                .map_err(|e| McpError::internal_error(format!("Search failed: {}", e), None))?
        };

        // Post-filter by optional criteria
        let mut results = self.get_item_summaries(&ids).await;

        if let Some(category) = &args.category {
            results.retain(|r| r.get("category").and_then(|v| v.as_str()) == Some(category));
        }
        if let Some(chips) = &args.chips {
            results.retain(|r| {
                let item_chips: Vec<String> = r.get("applies_to")
                    .and_then(|a| a.get("chips"))
                    .and_then(|c| serde_json::from_value(c.clone()).ok())
                    .unwrap_or_default();
                chips.iter().any(|c| item_chips.contains(c))
            });
        }
        if let Some(tags) = &args.tags {
            results.retain(|r| {
                let item_tags: Vec<String> = r.get("tags")
                    .and_then(|t| serde_json::from_value(t.clone()).ok())
                    .unwrap_or_default();
                tags.iter().any(|t| item_tags.contains(t))
            });
        }

        results.truncate(limit);

        let output = serde_json::json!({
            "query": args.query,
            "count": results.len(),
            "items": results,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output)
                .map_err(|e| McpError::internal_error(format!("JSON error: {}", e), None))?
        )]))
    }

    #[tool(description = "Get knowledge relevant to the current working context. Matches items by file patterns and optional board target. Use this during active development to surface relevant gotchas and learnings.")]
    async fn for_context(&self, Parameters(args): Parameters<ForContextArgs>) -> Result<CallToolResult, McpError> {
        let mut all_ids = Vec::new();

        // Match by file patterns
        {
            let db = self.db.lock().await;
            let file_ids = db.items_for_files(&args.files)
                .map_err(|e| McpError::internal_error(format!("File query failed: {}", e), None))?;
            all_ids.extend(file_ids);
        }

        // Match by board hierarchy
        if let Some(board_name) = &args.board {
            if let Some(board) = self.find_board(board_name).await {
                let db = self.db.lock().await;
                let board_ids = db.items_for_board(&board.board, &board.chip, &board.family, &board.arch)
                    .map_err(|e| McpError::internal_error(format!("Board query failed: {}", e), None))?;
                all_ids.extend(board_ids);
            }
        }

        // Deduplicate
        all_ids.sort();
        all_ids.dedup();

        let results = self.get_item_summaries(&all_ids).await;

        // Sort by severity (critical first)
        let mut sorted = results;
        sorted.sort_by(|a, b| {
            let sev_order = |s: &str| match s {
                "critical" => 0,
                "important" => 1,
                _ => 2,
            };
            let a_sev = a.get("severity").and_then(|v| v.as_str()).unwrap_or("informational");
            let b_sev = b.get("severity").and_then(|v| v.as_str()).unwrap_or("informational");
            sev_order(a_sev).cmp(&sev_order(b_sev))
        });

        let output = serde_json::json!({
            "context": {
                "files": args.files,
                "board": args.board,
            },
            "count": sorted.len(),
            "items": sorted,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output)
                .map_err(|e| McpError::internal_error(format!("JSON error: {}", e), None))?
        )]))
    }

    #[tool(description = "Mark a knowledge item as deprecated. Optionally specify the superseding item.")]
    async fn deprecate(&self, Parameters(args): Parameters<DeprecateArgs>) -> Result<CallToolResult, McpError> {
        let file_path = self.config.items_dir().join(format!("{}.yml", args.id));
        if !file_path.exists() {
            return Err(McpError::invalid_params(
                format!("Knowledge item not found: {}", args.id), None,
            ));
        }

        let mut item = KnowledgeItem::load(&file_path)
            .map_err(|e| McpError::internal_error(e, None))?;

        item.deprecated = true;
        item.superseded_by = args.superseded_by.clone();
        item.updated = chrono::Utc::now().format("%Y-%m-%d").to_string();

        item.save(&file_path)
            .map_err(|e| McpError::internal_error(e, None))?;

        // Re-index
        let hash = db::compute_item_hash(&item);
        {
            let db = self.db.lock().await;
            db.index_item(&item, &hash)
                .map_err(|e| McpError::internal_error(e, None))?;
        }
        {
            let mut items = self.items.write().await;
            items.insert(item.id.clone(), item);
        }

        let result = serde_json::json!({
            "id": args.id,
            "deprecated": true,
            "superseded_by": args.superseded_by,
            "message": "Knowledge item deprecated"
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result)
                .map_err(|e| McpError::internal_error(format!("JSON error: {}", e), None))?
        )]))
    }

    #[tool(description = "Mark a knowledge item as validated by an engineer. Items need validation before being auto-injected into context.")]
    async fn validate(&self, Parameters(args): Parameters<ValidateArgs>) -> Result<CallToolResult, McpError> {
        let file_path = self.config.items_dir().join(format!("{}.yml", args.id));
        if !file_path.exists() {
            return Err(McpError::invalid_params(
                format!("Knowledge item not found: {}", args.id), None,
            ));
        }

        let mut item = KnowledgeItem::load(&file_path)
            .map_err(|e| McpError::internal_error(e, None))?;

        if !item.validated_by.contains(&args.validated_by) {
            item.validated_by.push(args.validated_by.clone());
        }
        item.status = "validated".to_string();
        item.updated = chrono::Utc::now().format("%Y-%m-%d").to_string();

        item.save(&file_path)
            .map_err(|e| McpError::internal_error(e, None))?;

        // Re-index
        let hash = db::compute_item_hash(&item);
        {
            let db = self.db.lock().await;
            db.index_item(&item, &hash)
                .map_err(|e| McpError::internal_error(e, None))?;
        }
        {
            let mut items = self.items.write().await;
            items.insert(item.id.clone(), item);
        }

        let result = serde_json::json!({
            "id": args.id,
            "status": "validated",
            "validated_by": args.validated_by,
            "message": "Knowledge item validated"
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result)
                .map_err(|e| McpError::internal_error(format!("JSON error: {}", e), None))?
        )]))
    }

    #[tool(description = "Get recently created or updated knowledge items. Useful for session bootstrapping to see what's new.")]
    async fn recent(&self, Parameters(args): Parameters<RecentArgs>) -> Result<CallToolResult, McpError> {
        let days = args.days.unwrap_or(7);

        let ids = {
            let db = self.db.lock().await;
            db.recent_items(days)
                .map_err(|e| McpError::internal_error(format!("Recent query failed: {}", e), None))?
        };

        let results = self.get_item_summaries(&ids).await;

        let output = serde_json::json!({
            "days": days,
            "count": results.len(),
            "items": results,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output)
                .map_err(|e| McpError::internal_error(format!("JSON error: {}", e), None))?
        )]))
    }

    #[tool(description = "Find knowledge items that may be outdated. Returns items not updated within the specified number of days.")]
    async fn stale(&self, Parameters(args): Parameters<StaleArgs>) -> Result<CallToolResult, McpError> {
        let days = args.days.unwrap_or(90);

        let ids = {
            let db = self.db.lock().await;
            db.stale_items(days)
                .map_err(|e| McpError::internal_error(format!("Stale query failed: {}", e), None))?
        };

        let results = self.get_item_summaries(&ids).await;

        let output = serde_json::json!({
            "threshold_days": days,
            "count": results.len(),
            "items": results,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output)
                .map_err(|e| McpError::internal_error(format!("JSON error: {}", e), None))?
        )]))
    }

    #[tool(description = "List all tags/scopes used across knowledge items. Useful for discovering the vocabulary for search queries.")]
    async fn list_tags(&self, Parameters(args): Parameters<ListTagsArgs>) -> Result<CallToolResult, McpError> {
        let tags = {
            let db = self.db.lock().await;
            db.all_tags()
                .map_err(|e| McpError::internal_error(format!("Tags query failed: {}", e), None))?
        };

        let filtered: Vec<&String> = if let Some(prefix) = &args.prefix {
            tags.iter().filter(|t| t.starts_with(prefix.as_str())).collect()
        } else {
            tags.iter().collect()
        };

        let output = serde_json::json!({
            "count": filtered.len(),
            "tags": filtered,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output)
                .map_err(|e| McpError::internal_error(format!("JSON error: {}", e), None))?
        )]))
    }

    // ========================================================================
    // Board Profile tools
    // ========================================================================

    #[tool(description = "Get detailed information about a board: chip, flash method, memory, peripherals, errata, and related knowledge items.")]
    async fn board_info(&self, Parameters(args): Parameters<BoardInfoArgs>) -> Result<CallToolResult, McpError> {
        let board = self.find_board(&args.board).await
            .ok_or_else(|| McpError::invalid_params(
                format!("Board not found: {}. Use list_boards to see available boards.", args.board),
                None,
            ))?;

        // Get related knowledge items
        let ids = {
            let db = self.db.lock().await;
            db.items_for_board(&board.board, &board.chip, &board.family, &board.arch)
                .map_err(|e| McpError::internal_error(format!("Board query failed: {}", e), None))?
        };
        let knowledge = self.get_item_summaries(&ids).await;

        let output = serde_json::json!({
            "board": board.board,
            "chip": board.chip,
            "family": board.family,
            "arch": board.arch,
            "vendor": board.vendor,
            "flash_method": board.flash_method,
            "flash_tool": board.flash_tool,
            "probe": board.probe,
            "connect_under_reset": board.connect_under_reset,
            "target_chip": board.target_chip,
            "board_qualifier": board.board_qualifier,
            "memory": {
                "flash": board.memory.flash.as_ref().map(|f| serde_json::json!({
                    "type": f.flash_type,
                    "size_kb": f.size_kb,
                })),
                "ram": board.memory.ram.as_ref().map(|r| serde_json::json!({
                    "size_kb": r.size_kb,
                })),
            },
            "peripherals": board.peripherals,
            "features": board.features,
            "errata": board.known_errata.iter().map(|e| serde_json::json!({
                "id": e.id,
                "summary": e.summary,
                "workaround": e.workaround,
                "knowledge_items": e.knowledge_items,
            })).collect::<Vec<_>>(),
            "knowledge_items": knowledge,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output)
                .map_err(|e| McpError::internal_error(format!("JSON error: {}", e), None))?
        )]))
    }

    #[tool(description = "Get all knowledge items for a chip family. Resolves the chip to all boards using that chip and returns knowledge at every hierarchy level.")]
    async fn for_chip(&self, Parameters(args): Parameters<ForChipArgs>) -> Result<CallToolResult, McpError> {
        let boards = self.boards.read().await;

        // Find boards using this chip
        let matching_boards: Vec<&BoardProfile> = boards.iter()
            .filter(|b| b.chip == args.chip)
            .collect();

        if matching_boards.is_empty() {
            // Try as family name
            let family_boards: Vec<&BoardProfile> = boards.iter()
                .filter(|b| b.family == args.chip)
                .collect();
            if family_boards.is_empty() {
                return Err(McpError::invalid_params(
                    format!("No boards found for chip/family: {}", args.chip), None,
                ));
            }
        }

        // Collect all knowledge items across matching boards
        let mut all_ids = Vec::new();
        {
            let db = self.db.lock().await;
            for board in &matching_boards {
                let ids = db.items_for_board(&board.board, &board.chip, &board.family, &board.arch)
                    .map_err(|e| McpError::internal_error(e, None))?;
                all_ids.extend(ids);
            }
        }
        all_ids.sort();
        all_ids.dedup();

        let results = self.get_item_summaries(&all_ids).await;

        let output = serde_json::json!({
            "chip": args.chip,
            "boards": matching_boards.iter().map(|b| &b.board).collect::<Vec<_>>(),
            "count": results.len(),
            "items": results,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output)
                .map_err(|e| McpError::internal_error(format!("JSON error: {}", e), None))?
        )]))
    }

    #[tool(description = "Get board profile and all scoped knowledge for a board. Resolves hardware hierarchy to return knowledge at board, chip, family, and arch levels.")]
    async fn for_board(&self, Parameters(args): Parameters<ForBoardArgs>) -> Result<CallToolResult, McpError> {
        // Delegate to board_info which already does this
        self.board_info(Parameters(BoardInfoArgs { board: args.board })).await
    }

    #[tool(description = "List all available board profiles. Optionally filter by vendor.")]
    async fn list_boards(&self, Parameters(args): Parameters<ListBoardsArgs>) -> Result<CallToolResult, McpError> {
        let boards = self.boards.read().await;

        let filtered: Vec<&BoardProfile> = if let Some(vendor) = &args.vendor {
            boards.iter().filter(|b| b.vendor == *vendor).collect()
        } else {
            boards.iter().collect()
        };

        let output = serde_json::json!({
            "count": filtered.len(),
            "boards": filtered.iter().map(|b| serde_json::json!({
                "board": b.board,
                "chip": b.chip,
                "family": b.family,
                "vendor": b.vendor,
                "board_qualifier": b.board_qualifier,
            })).collect::<Vec<_>>(),
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output)
                .map_err(|e| McpError::internal_error(format!("JSON error: {}", e), None))?
        )]))
    }

    // ========================================================================
    // Auto-generation tools
    // ========================================================================

    #[tool(description = "Regenerate .claude/rules/*.md files from knowledge items. Groups items by file_patterns and generates one rules file per topic.")]
    async fn regenerate_rules(&self, Parameters(args): Parameters<RegenerateRulesArgs>) -> Result<CallToolResult, McpError> {
        let dry_run = args.dry_run.unwrap_or(false);
        let items = self.items.read().await;

        // Group items by their file patterns into topics
        // Items with overlapping patterns get grouped together
        let mut pattern_groups: HashMap<String, Vec<&KnowledgeItem>> = HashMap::new();

        for item in items.values() {
            if item.deprecated || item.file_patterns.is_empty() {
                continue;
            }
            // Use the first file pattern's "topic" as the group key
            let topic = infer_topic(item);
            pattern_groups.entry(topic).or_default().push(item);
        }

        let mut generated_files = Vec::new();

        for (topic, group_items) in &pattern_groups {
            // Collect all file patterns for this topic
            let mut all_patterns: Vec<String> = Vec::new();
            for item in group_items {
                all_patterns.extend(item.file_patterns.iter().cloned());
            }
            all_patterns.sort();
            all_patterns.dedup();

            // Generate rules file content
            let mut content = String::new();
            content.push_str("---\n");
            content.push_str(&format!("paths: {:?}\n", all_patterns));
            content.push_str("---\n");
            content.push_str(&format!("# {} Learnings\n\n", capitalize_topic(topic)));

            // Sort items: critical first, then important, then informational
            let mut sorted_items: Vec<&&KnowledgeItem> = group_items.iter().collect();
            sorted_items.sort_by(|a, b| {
                let sev_order = |s: &str| match s {
                    "critical" => 0,
                    "important" => 1,
                    _ => 2,
                };
                sev_order(&a.severity).cmp(&sev_order(&b.severity))
            });

            for item in sorted_items {
                content.push_str(&format!("- **{}** — {}\n", item.title, item.body.lines().next().unwrap_or("")));
            }

            let file_path = self.config.rules_dir().join(format!("{}.md", topic));
            generated_files.push(serde_json::json!({
                "topic": topic,
                "file": file_path.display().to_string(),
                "patterns": all_patterns,
                "item_count": group_items.len(),
            }));

            if !dry_run {
                if let Some(parent) = file_path.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                std::fs::write(&file_path, &content)
                    .map_err(|e| McpError::internal_error(
                        format!("Failed to write {}: {}", file_path.display(), e), None,
                    ))?;
                info!("Generated rules file: {}", file_path.display());
            }
        }

        let output = serde_json::json!({
            "dry_run": dry_run,
            "files_generated": generated_files.len(),
            "files": generated_files,
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output)
                .map_err(|e| McpError::internal_error(format!("JSON error: {}", e), None))?
        )]))
    }

    #[tool(description = "Regenerate the Key Gotchas section content from severity=critical, status=validated knowledge items. Returns the markdown content to update in CLAUDE.md.")]
    async fn regenerate_gotchas(&self, Parameters(args): Parameters<RegenerateGotchasArgs>) -> Result<CallToolResult, McpError> {
        let _dry_run = args.dry_run.unwrap_or(false);
        let items = self.items.read().await;

        // Collect critical, validated items
        let mut gotcha_items: Vec<&KnowledgeItem> = items.values()
            .filter(|i| i.severity == "critical" && !i.deprecated)
            .collect();

        gotcha_items.sort_by(|a, b| a.title.cmp(&b.title));

        let mut content = String::new();
        content.push_str("## Key Gotchas\n\n");
        content.push_str("Hard-won lessons (Tier 1 — always loaded). Full details in individual `knowledge/items/` files.\n\n");

        for item in &gotcha_items {
            let first_line = item.body.lines().next().unwrap_or("");
            content.push_str(&format!("- **{}**: {}\n", item.title, first_line));
        }

        let output = serde_json::json!({
            "gotcha_count": gotcha_items.len(),
            "content": content,
            "items": gotcha_items.iter().map(|i| serde_json::json!({
                "id": i.id,
                "title": i.title,
                "severity": i.severity,
            })).collect::<Vec<_>>(),
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&output)
                .map_err(|e| McpError::internal_error(format!("JSON error: {}", e), None))?
        )]))
    }
}

#[tool_handler]
impl ServerHandler for KnowledgeToolHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Knowledge Server MCP - Structured knowledge management with hardware-aware retrieval. \
                 14 tools: capture, search, for_context, deprecate, validate, recent, stale, list_tags, \
                 board_info, for_chip, for_board, list_boards, regenerate_rules, regenerate_gotchas."
                    .to_string(),
            ),
        }
    }
}

/// Infer a topic name from a knowledge item's file patterns and scope.
fn infer_topic(item: &KnowledgeItem) -> String {
    // Check boards/chips first
    if !item.applies_to.boards.is_empty() {
        let board = &item.applies_to.boards[0];
        if board.contains("nrf54l15") { return "nrf54l15".to_string(); }
        if board.contains("nrf52840") { return "nrf52840".to_string(); }
        if board.contains("esp32") { return "esp32".to_string(); }
    }

    // Check file patterns for topic clues
    for pattern in &item.file_patterns {
        if pattern.contains("coredump") || pattern.contains("crash") || pattern.contains("dump") {
            return "coredump".to_string();
        }
        if pattern.contains("CMakeLists") || pattern.contains("cmake") || pattern.contains("Kconfig") || pattern.contains("module.yml") {
            return "build-system".to_string();
        }
        if pattern.contains("rtt") || pattern.contains("RTT") {
            return "rtt".to_string();
        }
        if pattern.contains("test") || pattern.contains("twister") {
            return "testing".to_string();
        }
    }

    // Check subsystems
    for sub in &item.applies_to.subsystems {
        match sub.as_str() {
            "coredump" | "crash" => return "coredump".to_string(),
            "shell" => return "shell".to_string(),
            "bluetooth" | "ble" => return "bluetooth".to_string(),
            "logging" | "rtt" => return "rtt".to_string(),
            "build-system" | "cmake" | "kconfig" => return "build-system".to_string(),
            "testing" | "twister" => return "testing".to_string(),
            _ => {}
        }
    }

    // Check tags as fallback
    for tag in &item.tags {
        match tag.as_str() {
            "coredump" => return "coredump".to_string(),
            "rtt" => return "rtt".to_string(),
            "testing" | "twister" => return "testing".to_string(),
            "build-system" | "cmake" | "kconfig" => return "build-system".to_string(),
            "nrf54l15" => return "nrf54l15".to_string(),
            "nrf52840" => return "nrf52840".to_string(),
            _ => {}
        }
    }

    // Default: use category
    item.category.clone()
}

fn capitalize_topic(topic: &str) -> String {
    topic.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
