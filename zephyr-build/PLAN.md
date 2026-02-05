# Plan: Zephyr Build MCP Server

## Overview

Create a new MCP server at `/Users/danahern/code/claude/work/claude-mcps/zephyr-build` that handles Zephyr application building. This offloads build operations from the main Claude context, allowing builds to run without consuming conversation context for actual problem solving.

## Architecture

```
┌─────────────────────────────────────────┐
│         MCP Tools Layer (5 tools)       │
├─────────────────────────────────────────┤
│ • list_apps    - Discover applications  │
│ • list_boards  - Available board targets│
│ • build        - West build wrapper     │
│ • clean        - Remove build artifacts │
│ • build_status - Background build state │
└─────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│           West CLI (subprocess)         │
│  west build -b <board> apps/<app>       │
└─────────────────────────────────────────┘
```

**Design Principles:**
- Thin wrapper around `west` commands (subprocess, not library)
- Auto-detect workspace path from config or environment
- Support background builds with status polling
- Mirror embedded-probe patterns for consistency

---

## Tools (5 total)

### 1. `list_apps` - List Available Applications
Scan the apps/ directory for valid Zephyr applications.

```rust
ListAppsArgs {
    workspace_path: Option<String>,  // Override default workspace
}

ListAppsResult {
    apps: Vec<AppInfo>,
}

AppInfo {
    name: String,           // e.g., "ble_wifi_bridge"
    path: String,           // e.g., "apps/ble_wifi_bridge"
    has_build: bool,        // Build directory exists
    board: Option<String>,  // Board from last build (if exists)
}
```

### 2. `list_boards` - List Supported Boards
Return commonly-used boards with optional west board lookup.

```rust
ListBoardsArgs {
    filter: Option<String>,     // Optional filter pattern
    include_all: bool,          // Include all west boards (slow)
}

ListBoardsResult {
    boards: Vec<BoardInfo>,
}

BoardInfo {
    name: String,               // e.g., "nrf52840dk/nrf52840"
    arch: String,               // e.g., "arm"
    vendor: Option<String>,     // e.g., "Nordic"
}
```

### 3. `build` - Build Zephyr Application
Run west build for an application and board.

```rust
BuildArgs {
    app: String,                // App name or path
    board: String,              // Board identifier
    pristine: bool,             // --pristine flag (default: false)
    extra_args: Option<String>, // Additional west/cmake args
    background: bool,           // Run in background (default: false)
    workspace_path: Option<String>,
}

BuildResult {
    success: bool,
    build_id: Option<String>,   // For background builds
    output: String,             // Build output (if not background)
    artifact_path: Option<String>, // Path to zephyr.elf/bin/hex
    duration_ms: Option<u64>,
}
```

### 4. `clean` - Clean Build Artifacts
Remove build directory for an application.

```rust
CleanArgs {
    app: String,
    workspace_path: Option<String>,
}

CleanResult {
    success: bool,
    message: String,
}
```

### 5. `build_status` - Check Background Build Status
Poll status of a background build.

```rust
BuildStatusArgs {
    build_id: String,
}

BuildStatusResult {
    status: String,             // "running", "complete", "failed"
    progress: Option<String>,   // Current build phase if available
    output: Option<String>,     // Build output if complete
    artifact_path: Option<String>,
    error: Option<String>,
}
```

---

## File Structure

```
claude-mcps/zephyr-build/
├── Cargo.toml
├── README.md
└── src/
    ├── main.rs           # Entry point, stdio transport
    ├── lib.rs            # Re-exports
    ├── config.rs         # Configuration loading
    ├── error.rs          # Error types
    └── tools/
        ├── mod.rs        # Tool router
        ├── types.rs      # Args/Result structs
        └── build_tools.rs # Tool implementations
```

**Rationale:** Mirrors embedded-probe structure for consistency. All tools in single file to minimize maintenance.

---

## Implementation Details

### Workspace Detection
```rust
fn find_workspace() -> Result<PathBuf> {
    // 1. Check config file
    // 2. Check ZEPHYR_WORKSPACE env var
    // 3. Look for .west/ in current or parent directories
    // 4. Error if not found
}
```

### Build Command
```rust
// west build -b nrf52840dk/nrf52840 apps/ble_wifi_bridge --pristine
Command::new("west")
    .args(["build", "-b", &args.board])
    .arg(&app_path)
    .args(if args.pristine { vec!["--pristine"] } else { vec![] })
    .current_dir(&workspace)
    .output()
```

### Background Builds
Use tokio::spawn with a HashMap<String, BuildState> to track running builds:
```rust
struct BuildState {
    status: BuildStatus,
    output: String,
    started_at: Instant,
}

enum BuildStatus {
    Running,
    Complete { artifact_path: PathBuf },
    Failed { error: String },
}
```

### Board List (Hardcoded Common + Optional Full List)
```rust
const COMMON_BOARDS: &[(&str, &str, &str)] = &[
    ("nrf52840dk/nrf52840", "arm", "Nordic"),
    ("nrf5340dk/nrf5340/cpuapp", "arm", "Nordic"),
    ("esp32_devkitc/esp32/procpu", "xtensa", "Espressif"),
    ("esp32s3_eye/esp32s3/procpu", "xtensa", "Espressif"),
    ("esp32c3_devkitc", "riscv", "Espressif"),
    ("stm32f4_disco", "arm", "ST"),
    ("nucleo_f411re", "arm", "ST"),
    ("native_sim", "posix", "Zephyr"),
];
```

Full board list via: `west boards` (slow, optional)

---

## Dependencies

```toml
[package]
name = "zephyr-build"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "zephyr-build"
path = "src/main.rs"

[lib]
name = "zephyr_build"
path = "src/lib.rs"

[dependencies]
rmcp = { version = "0.3.2", features = ["server", "macros", "transport-io"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
clap = { version = "4", features = ["derive"] }
toml = "0.8"
uuid = { version = "1", features = ["v4"] }  # For build IDs
```

---

## Configuration

```toml
# ~/.config/zephyr-build/config.toml
[workspace]
path = "/Users/danahern/code/claude/work"

[build]
default_pristine = false
```

Also supports:
- `--workspace` CLI argument
- `ZEPHYR_WORKSPACE` environment variable

---

## Verification Plan

### 1. list_apps
```
list_apps()
→ [{ name: "ble_wifi_bridge", path: "apps/ble_wifi_bridge", ... },
   { name: "ble_data_transfer", path: "apps/ble_data_transfer", ... }]
```

### 2. list_boards
```
list_boards(filter="nrf")
→ [{ name: "nrf52840dk/nrf52840", arch: "arm", vendor: "Nordic" }, ...]
```

### 3. build
```
build(app="ble_wifi_bridge", board="nrf52840dk/nrf52840", pristine=true)
→ { success: true, artifact_path: "apps/ble_wifi_bridge/build/zephyr/zephyr.elf", ... }
```

### 4. clean
```
clean(app="ble_wifi_bridge")
→ { success: true, message: "Removed build directory" }
```

### 5. build (background) + build_status
```
build(app="ble_wifi_bridge", board="nrf52840dk/nrf52840", background=true)
→ { build_id: "abc123", ... }

build_status(build_id="abc123")
→ { status: "running", progress: "Compiling...", ... }
```

---

## Implementation Order

1. **Scaffolding** - Create Cargo.toml, main.rs, config.rs, error.rs
2. **list_apps** - Scan apps/ directory (simplest tool)
3. **list_boards** - Return hardcoded list + optional west boards
4. **build** - Synchronous west build wrapper
5. **clean** - Remove build directory
6. **build (background) + build_status** - Add async support

---

## Files to Create

| File | Purpose |
|------|---------|
| `claude-mcps/zephyr-build/Cargo.toml` | Package manifest |
| `claude-mcps/zephyr-build/src/main.rs` | Entry point |
| `claude-mcps/zephyr-build/src/lib.rs` | Library re-exports |
| `claude-mcps/zephyr-build/src/config.rs` | Configuration loading |
| `claude-mcps/zephyr-build/src/error.rs` | Error types |
| `claude-mcps/zephyr-build/src/tools/mod.rs` | Tool router |
| `claude-mcps/zephyr-build/src/tools/types.rs` | Arg/Result structs |
| `claude-mcps/zephyr-build/src/tools/build_tools.rs` | Tool implementations |
| `claude-mcps/zephyr-build/README.md` | Documentation |

---

## Integration

Update `claude-mcps/README.md` to include zephyr-build MCP.

Configure in Claude Code settings:
```json
{
  "mcpServers": {
    "zephyr-build": {
      "command": "/path/to/claude-mcps/zephyr-build/target/release/zephyr-build",
      "args": ["--workspace", "/path/to/workspace"]
    }
  }
}
```
