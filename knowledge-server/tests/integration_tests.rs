use knowledge_server::knowledge::KnowledgeItem;
use knowledge_server::db::KnowledgeDb;
use knowledge_server::tools::snippet;
use std::path::Path;

#[test]
#[ignore] // Requires workspace root knowledge/boards/ directory
fn test_load_board_profiles() {
    let boards_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap()
        .join("knowledge").join("boards");

    if !boards_dir.exists() {
        panic!("Boards directory not found at {:?}", boards_dir);
    }

    let boards = knowledge_server::boards::load_all_boards(&boards_dir).unwrap();
    assert!(boards.len() >= 4, "Expected at least 4 board profiles, got {}", boards.len());

    // Verify nrf54l15dk
    let nrf54 = boards.iter().find(|b| b.board == "nrf54l15dk").unwrap();
    assert_eq!(nrf54.chip, "nrf54l15");
    assert_eq!(nrf54.family, "nrf54");
    assert_eq!(nrf54.arch, "arm-cortex-m");
    assert_eq!(nrf54.vendor, "nordic");
    assert_eq!(nrf54.flash_method, "hex");
    assert!(nrf54.connect_under_reset);
    assert_eq!(nrf54.board_qualifier.as_deref(), Some("nrf54l15dk/nrf54l15/cpuapp"));

    // Verify memory
    let flash = nrf54.memory.flash.as_ref().unwrap();
    assert_eq!(flash.flash_type, "rram");
    assert_eq!(flash.size_kb, 1524);

    // Verify qemu
    let qemu = boards.iter().find(|b| b.board == "qemu_cortex_m3").unwrap();
    assert_eq!(qemu.vendor, "qemu");
    assert!(qemu.features.contains(&"unit-testing".to_string()));
}

#[test]
#[ignore] // Requires workspace root knowledge/items/ directory
fn test_load_knowledge_items() {
    let items_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap()
        .join("knowledge").join("items");

    if !items_dir.exists() {
        panic!("Items directory not found at {:?}", items_dir);
    }

    let items = knowledge_server::knowledge::load_all_items(&items_dir).unwrap();
    assert!(items.len() >= 24, "Expected at least 24 items, got {}", items.len());

    // Verify a specific item
    let nrf_item = items.iter().find(|i| i.title.contains("RRAM flash quirks")).unwrap();
    assert_eq!(nrf_item.category, "hardware");
    assert_eq!(nrf_item.severity, "critical");
    assert!(nrf_item.applies_to.boards.contains(&"nrf54l15dk".to_string()));
    assert!(nrf_item.applies_to.chips.contains(&"nrf54l15".to_string()));
    assert!(nrf_item.applies_to.tools.contains(&"probe-rs".to_string()));
    assert_eq!(nrf_item.status, "validated");
}

#[test]
fn test_knowledge_item_generate_id() {
    let id = KnowledgeItem::generate_id();
    assert!(id.starts_with("k-"), "ID should start with 'k-': {}", id);
    // UUID v4 format: k-xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx
    assert_eq!(id.len(), 2 + 36, "ID should be 'k-' + 36-char UUID: {}", id);
    // Verify uniqueness
    let id2 = KnowledgeItem::generate_id();
    assert_ne!(id, id2, "Two generated IDs should be unique");
}

#[test]
fn test_db_index_and_search() {
    let db = KnowledgeDb::open_memory().unwrap();

    let item = KnowledgeItem {
        id: "k-test-001".to_string(),
        title: "nRF54L15 RRAM flash quirks".to_string(),
        body: "Use .hex not .elf for flashing on nRF54L15 RRAM".to_string(),
        category: "hardware".to_string(),
        severity: "critical".to_string(),
        applies_to: knowledge_server::knowledge::AppliesTo {
            boards: vec!["nrf54l15dk".to_string()],
            chips: vec!["nrf54l15".to_string()],
            tools: vec!["probe-rs".to_string()],
            subsystems: vec![],
        },
        file_patterns: vec!["**/*nrf54l15*".to_string()],
        status: "validated".to_string(),
        validated_by: vec!["danahern".to_string()],
        deprecated: false,
        superseded_by: None,
        created: "2026-02-14".to_string(),
        updated: "2026-02-14".to_string(),
        author: "danahern".to_string(),
        source_session: None,
        tags: vec!["nrf54l15".to_string(), "flashing".to_string()],
    };

    db.index_item(&item, "testhash123").unwrap();

    // Search by keyword
    let results = db.search("nrf54l15 flashing", 10).unwrap();
    assert!(!results.is_empty(), "Search for 'nrf54l15 flashing' should return results");
    assert!(results.contains(&"k-test-001".to_string()));

    // Search by RRAM
    let results = db.search("RRAM", 10).unwrap();
    assert!(results.contains(&"k-test-001".to_string()));
}

#[test]
fn test_db_items_for_board() {
    let db = KnowledgeDb::open_memory().unwrap();

    let item = KnowledgeItem {
        id: "k-board-001".to_string(),
        title: "Test board item".to_string(),
        body: "Test body".to_string(),
        category: "hardware".to_string(),
        severity: "critical".to_string(),
        applies_to: knowledge_server::knowledge::AppliesTo {
            boards: vec!["nrf54l15dk".to_string()],
            chips: vec!["nrf54l15".to_string()],
            tools: vec![],
            subsystems: vec![],
        },
        file_patterns: vec![],
        status: "validated".to_string(),
        validated_by: vec![],
        deprecated: false,
        superseded_by: None,
        created: "2026-02-14".to_string(),
        updated: "2026-02-14".to_string(),
        author: "test".to_string(),
        source_session: None,
        tags: vec![],
    };

    db.index_item(&item, "hash").unwrap();

    // Should find by exact board
    let results = db.items_for_board("nrf54l15dk", "nrf54l15", "nrf54", "arm-cortex-m").unwrap();
    assert!(results.contains(&"k-board-001".to_string()));
}

#[test]
fn test_db_items_for_files() {
    let db = KnowledgeDb::open_memory().unwrap();

    let item = KnowledgeItem {
        id: "k-file-001".to_string(),
        title: "Coredump item".to_string(),
        body: "Test body".to_string(),
        category: "operational".to_string(),
        severity: "important".to_string(),
        applies_to: knowledge_server::knowledge::AppliesTo::default(),
        file_patterns: vec!["**/*crash*".to_string(), "**/*coredump*".to_string()],
        status: "validated".to_string(),
        validated_by: vec![],
        deprecated: false,
        superseded_by: None,
        created: "2026-02-14".to_string(),
        updated: "2026-02-14".to_string(),
        author: "test".to_string(),
        source_session: None,
        tags: vec![],
    };

    db.index_item(&item, "hash").unwrap();

    // Should match by file pattern
    let results = db.items_for_files(&["src/crash_handler.c".to_string()]).unwrap();
    assert!(results.contains(&"k-file-001".to_string()));

    // Should not match unrelated file
    let results = db.items_for_files(&["src/main.c".to_string()]).unwrap();
    assert!(!results.contains(&"k-file-001".to_string()));
}

#[test]
fn test_db_recent_and_stale() {
    let db = KnowledgeDb::open_memory().unwrap();

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let recent_item = KnowledgeItem {
        id: "k-recent-001".to_string(),
        title: "Recent item".to_string(),
        body: "Test".to_string(),
        category: "operational".to_string(),
        severity: "informational".to_string(),
        applies_to: knowledge_server::knowledge::AppliesTo::default(),
        file_patterns: vec![],
        status: "validated".to_string(),
        validated_by: vec![],
        deprecated: false,
        superseded_by: None,
        created: today.clone(),
        updated: today.clone(),
        author: "test".to_string(),
        source_session: None,
        tags: vec![],
    };

    let old_item = KnowledgeItem {
        id: "k-old-001".to_string(),
        title: "Old item".to_string(),
        body: "Test".to_string(),
        category: "operational".to_string(),
        severity: "informational".to_string(),
        applies_to: knowledge_server::knowledge::AppliesTo::default(),
        file_patterns: vec![],
        status: "validated".to_string(),
        validated_by: vec![],
        deprecated: false,
        superseded_by: None,
        created: "2025-01-01".to_string(),
        updated: "2025-01-01".to_string(),
        author: "test".to_string(),
        source_session: None,
        tags: vec![],
    };

    db.index_item(&recent_item, "hash1").unwrap();
    db.index_item(&old_item, "hash2").unwrap();

    // Recent should only include today's item
    let recent = db.recent_items(7).unwrap();
    assert!(recent.contains(&"k-recent-001".to_string()));
    assert!(!recent.contains(&"k-old-001".to_string()));

    // Stale should only include old item
    let stale = db.stale_items(30).unwrap();
    assert!(stale.contains(&"k-old-001".to_string()));
    assert!(!stale.contains(&"k-recent-001".to_string()));
}

#[test]
fn test_db_all_tags() {
    let db = KnowledgeDb::open_memory().unwrap();

    let item = KnowledgeItem {
        id: "k-tags-001".to_string(),
        title: "Tagged item".to_string(),
        body: "Test".to_string(),
        category: "operational".to_string(),
        severity: "informational".to_string(),
        applies_to: knowledge_server::knowledge::AppliesTo {
            boards: vec!["nrf54l15dk".to_string()],
            chips: vec![],
            tools: vec!["probe-rs".to_string()],
            subsystems: vec![],
        },
        file_patterns: vec![],
        status: "validated".to_string(),
        validated_by: vec![],
        deprecated: false,
        superseded_by: None,
        created: "2026-02-14".to_string(),
        updated: "2026-02-14".to_string(),
        author: "test".to_string(),
        source_session: None,
        tags: vec!["flashing".to_string(), "rram".to_string()],
    };

    db.index_item(&item, "hash").unwrap();

    let tags = db.all_tags().unwrap();
    assert!(tags.contains(&"flashing".to_string()));
    assert!(tags.contains(&"rram".to_string()));
    assert!(tags.contains(&"nrf54l15dk".to_string()));
    assert!(tags.contains(&"probe-rs".to_string()));
}

#[test]
fn test_db_rebuild() {
    let db = KnowledgeDb::open_memory().unwrap();

    let items: Vec<KnowledgeItem> = (1..=5).map(|i| KnowledgeItem {
        id: format!("k-rebuild-{:03}", i),
        title: format!("Item {}", i),
        body: format!("Body for item {}", i),
        category: "operational".to_string(),
        severity: "informational".to_string(),
        applies_to: knowledge_server::knowledge::AppliesTo::default(),
        file_patterns: vec![],
        status: "validated".to_string(),
        validated_by: vec![],
        deprecated: false,
        superseded_by: None,
        created: "2026-02-14".to_string(),
        updated: "2026-02-14".to_string(),
        author: "test".to_string(),
        source_session: None,
        tags: vec![],
    }).collect();

    db.rebuild(&items).unwrap();

    let all_ids = db.all_item_ids().unwrap();
    assert_eq!(all_ids.len(), 5);
}

#[test]
#[ignore] // Requires workspace root knowledge/boards/ directory
fn test_board_hierarchy() {
    let boards_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap()
        .join("knowledge").join("boards");

    let boards = knowledge_server::boards::load_all_boards(&boards_dir).unwrap();
    let nrf54 = boards.iter().find(|b| b.board == "nrf54l15dk").unwrap();

    let hierarchy = nrf54.hierarchy();
    assert_eq!(hierarchy[0], ("board", "nrf54l15dk"));
    assert_eq!(hierarchy[1], ("chip", "nrf54l15"));
    assert_eq!(hierarchy[2], ("family", "nrf54"));
    assert_eq!(hierarchy[3], ("arch", "arm-cortex-m"));
}

#[test]
fn test_knowledge_item_searchable_text() {
    let item = KnowledgeItem {
        id: "k-test-001".to_string(),
        title: "Test item".to_string(),
        body: "Some body text about RRAM".to_string(),
        category: "hardware".to_string(),
        severity: "critical".to_string(),
        applies_to: knowledge_server::knowledge::AppliesTo {
            boards: vec!["nrf54l15dk".to_string()],
            chips: vec![],
            tools: vec!["probe-rs".to_string()],
            subsystems: vec![],
        },
        file_patterns: vec![],
        status: "validated".to_string(),
        validated_by: vec![],
        deprecated: false,
        superseded_by: None,
        created: "2026-02-14".to_string(),
        updated: "2026-02-14".to_string(),
        author: "test".to_string(),
        source_session: None,
        tags: vec!["flashing".to_string()],
    };

    let text = item.searchable_text();
    assert!(text.contains("Test item"));
    assert!(text.contains("RRAM"));
    assert!(text.contains("hardware"));
    assert!(text.contains("nrf54l15dk"));
    assert!(text.contains("probe-rs"));
    assert!(text.contains("flashing"));
}

#[test]
fn test_snippet_short_body() {
    assert_eq!(snippet("Short body."), "Short body.");
}

#[test]
fn test_snippet_first_sentence() {
    assert_eq!(
        snippet("First sentence. Second sentence. Third sentence."),
        "First sentence."
    );
}

#[test]
fn test_snippet_newline_paragraph() {
    assert_eq!(
        snippet("First paragraph.\n\nSecond paragraph."),
        "First paragraph."
    );
}

#[test]
fn test_snippet_dot_newline() {
    assert_eq!(
        snippet("First line.\nSecond line."),
        "First line."
    );
}

#[test]
fn test_snippet_truncates_long_sentence() {
    let long = "A ".repeat(100); // 200 chars, no sentence boundary
    let result = snippet(&long);
    assert!(result.len() <= 155, "snippet too long: {} chars", result.len());
    assert!(result.ends_with("..."));
}

#[test]
fn test_snippet_no_sentence_boundary_short() {
    assert_eq!(snippet("No period here"), "No period here");
}
