# knowledge-server

MCP server for structured knowledge management with hardware-aware retrieval. Provides full-text search over knowledge items, board profile lookups, and auto-generation of Claude Code rules files.

## Build

```bash
cargo build --release
```

## Configuration

Pass `--workspace` (workspace root containing `knowledge/`, `.cache/`, `.claude/`) at server start. Defaults to current directory.

## Storage

- **Source of truth**: `knowledge/items/*.yml` (git-tracked YAML files)
- **Board profiles**: `knowledge/boards/*.yml` (git-tracked)
- **Query cache**: `.cache/index.db` (gitignored SQLite with FTS5, rebuilt on startup)

## Tools

### Knowledge Store

#### `capture`
Create a new knowledge item. Auto-generates ID and indexes for search.
- `title` (required) — Title of the learning
- `body` (required) — The actual content
- `category` — hardware, toolchain, pattern, operational
- `severity` — critical, important, informational
- `boards`, `chips`, `tools`, `subsystems` — Scope arrays
- `file_patterns` — Glob patterns for auto-injection triggers
- `tags` — Search tags
- `author` — Who captured this

#### `search`
Full-text search using SQLite FTS5. Supports AND, OR, NOT, phrases.
- `query` (required) — FTS5 search query
- `tags`, `chips`, `category` — Optional filters
- `limit` — Max results (default: 20)

#### `for_context`
Get knowledge relevant to current files and build target.
- `files` (required) — File paths being worked on
- `board` — Board being targeted (triggers hierarchy resolution)

#### `deprecate`
Mark a knowledge item as deprecated.
- `id` (required) — Item ID
- `superseded_by` — ID of replacement item

#### `validate`
Mark a knowledge item as validated by an engineer.
- `id` (required) — Item ID
- `validated_by` (required) — Engineer name

#### `recent`
Items created/updated in last N days.
- `days` — Lookback period (default: 7)

#### `stale`
Items not updated in N+ days.
- `days` — Staleness threshold (default: 90)

#### `list_tags`
All tags/scopes across knowledge items.
- `prefix` — Filter by prefix

### Board Profiles

#### `board_info`
Full board details + related knowledge items.
- `board` (required) — Board name (e.g., "nrf54l15dk")

#### `for_chip`
All knowledge for a chip family via hierarchy resolution.
- `chip` (required) — Chip name (e.g., "nrf54l15")

#### `for_board`
Board profile + scoped knowledge (alias for board_info).
- `board` (required) — Board name

#### `list_boards`
All available board profiles.
- `vendor` — Filter by vendor

### Auto-Generation

#### `regenerate_rules`
Rebuild `.claude/rules/*.md` from knowledge items grouped by file patterns.
- `dry_run` — Preview without writing (default: false)

#### `regenerate_gotchas`
Generate Key Gotchas section content from critical knowledge items.
- `dry_run` — Preview without writing (default: false)

## Source Layout

```
src/
├── main.rs          # Entry point, logging init
├── lib.rs           # Public exports
├── config.rs        # CLI args (clap) + Config struct
├── db.rs            # SQLite index with FTS5
├── knowledge.rs     # Knowledge item types, YAML I/O
├── boards.rs        # Board profile types, YAML I/O
└── tools/
    ├── mod.rs       # Module exports
    ├── types.rs     # All MCP tool arg structs
    └── handler.rs   # Tool router + handler (14 tools)
```
