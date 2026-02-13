# Plan: ESP-IDF Build MCP Server

## Overview

MCP server at `claude-mcps/esp-idf-build` that wraps `idf.py` for ESP-IDF project building, flashing, and monitoring. Offloads build operations from the main Claude context.

## Architecture

```
┌─────────────────────────────────────────┐
│        MCP Tools Layer (8 tools)        │
├─────────────────────────────────────────┤
│ • list_projects - Discover projects     │
│ • list_targets  - Supported ESP32 chips │
│ • set_target    - Configure chip target │
│ • build         - idf.py build wrapper  │
│ • flash         - idf.py flash wrapper  │
│ • monitor       - Serial output capture │
│ • clean         - Remove build artifacts│
│ • build_status  - Background build state│
└─────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│         idf.py CLI (subprocess)         │
└─────────────────────────────────────────┘
```

## File Structure

```
claude-mcps/esp-idf-build/
├── Cargo.toml
├── PLAN.md
├── README.md
├── src/
│   ├── main.rs           # Entry point, stdio transport
│   ├── lib.rs            # Re-exports
│   ├── config.rs         # Configuration loading
│   └── tools/
│       ├── mod.rs        # Tool router
│       ├── types.rs      # Args/Result structs
│       └── build_tools.rs # Tool implementations
└── tests/
    └── integration_tests.rs
```

## Testing

### Running Tests

```bash
cd claude-mcps/esp-idf-build && cargo test
```

All tests run without hardware, ESP-IDF installation, or connected devices.

### Test Coverage (16 tests)

**Unit tests** (`src/main.rs` — 5 tests):
- `test_args_parsing_defaults` — Default CLI args (all None)
- `test_args_parsing_with_options` — `--idf-path`, `--projects-dir`, `--port`, `--log-level`
- `test_default_config` — `Config::default()` has all None fields
- `test_config_from_args` — Config propagates idf_path and port from args
- `test_config_from_args_no_options` — Config with no CLI options

**Tool tests** (`src/tools/build_tools.rs` — 6 tests):
- `test_list_targets` — Returns known ESP32 targets with correct fields
- `test_list_targets_has_arch` — Each target has architecture info (xtensa/riscv)
- `test_list_projects_empty_dir` — Empty directory returns empty project list
- `test_list_projects_with_projects` — Discovers projects via CMakeLists.txt with `project()` call
- `test_list_projects_no_dir` — Missing directory returns error
- `test_build_status_unknown_id` — Unknown build ID returns error

**Integration tests** (`tests/integration_tests.rs` — 5 tests):
- `test_handler_creation` — Handler with default config
- `test_handler_default` — Default trait implementation
- `test_config_default_values` — Config field defaults
- `test_config_from_args` — Config from CLI args with all fields
- `test_multiple_handlers` — Multiple handler instances with different configs

## Implementation Status

**COMPLETE** — All 8 tools implemented and building.
