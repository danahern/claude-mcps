# PRD: esp-idf-build MCP Server

## Purpose

Build, flash, and monitor server for ESP-IDF projects across all ESP32 chip variants. Wraps `idf.py` so Claude Code can manage the full ESP32 development lifecycle — set target, build, flash multi-segment firmware, and capture serial output — without the developer running CLI commands.

## Technology Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Language | Rust | Consistent with other MCP servers, fast startup |
| MCP SDK | rmcp 0.3.2 | Official Rust MCP SDK |
| Build Tool | idf.py (subprocess) | Standard ESP-IDF build/flash/monitor tool |
| Async | tokio 1 | Background build support |

## Tools (8)

| Tool | Args | Returns |
|------|------|---------|
| `list_projects` | projects_dir | Array of {name, path, has_build} |
| `list_targets` | — | Array of {name, architecture, description} |
| `set_target` | project, target, projects_dir | Success confirmation |
| `build` | project, background, projects_dir | {success, output, artifact_path, duration_ms} or {build_id} |
| `flash` | project, port, baud, projects_dir | Flash result with all segments |
| `monitor` | project, port, duration_seconds, projects_dir | Captured serial output |
| `clean` | project, projects_dir | Clean confirmation |
| `build_status` | build_id | {status, progress, output, error} |

### list_projects

Scans the configured projects directory for subdirectories containing `CMakeLists.txt` with a `project()` call. Distinguishes ESP-IDF projects from other CMake projects.

### list_targets

Returns all 10 supported ESP32 variants with architecture (Xtensa or RISC-V) and key features. No args needed — this is a static list.

### set_target

Runs `idf.py -C <project> set-target <target>`. Modifies the project's `sdkconfig` for the specified chip. Must be called before first build or when switching chips.

### flash

Runs `idf.py -C <project> flash -p <port>`. Automatically handles multi-segment flashing: bootloader + partition table + application binary. No manual address specification needed.

### monitor

Runs `idf.py -C <project> monitor -p <port>` for a fixed duration (default 10 seconds), then kills the process and returns captured output. Not interactive — designed for boot log capture and runtime verification.

## Supported Targets

| Target | Architecture | Key Features |
|--------|-------------|--------------|
| esp32 | Xtensa LX6 | Dual-core, WiFi + BLE |
| esp32s2 | Xtensa LX7 | Single-core, WiFi, USB-OTG |
| esp32s3 | Xtensa LX7 | Dual-core, WiFi + BLE, AI acceleration |
| esp32c2 | RISC-V | Low-cost WiFi + BLE |
| esp32c3 | RISC-V | WiFi + BLE |
| esp32c5 | RISC-V | WiFi 6 + BLE |
| esp32c6 | RISC-V | WiFi 6 + BLE + 802.15.4 (Thread/Zigbee) |
| esp32c61 | RISC-V | WiFi 6 + BLE |
| esp32h2 | RISC-V | BLE + 802.15.4 (Thread/Zigbee) |
| esp32p4 | RISC-V | Dual-core high-performance |

## Architecture

```
┌────────────────────────────┐
│  MCP Tool Layer (8 tools)  │
├────────────────────────────┤
│  Background Build Manager  │
│  HashMap<UUID, BuildState> │
│  tokio::spawn per build    │
├────────────────────────────┤
│  IDF Environment Manager   │
│  Source once, cache result  │
├────────────────────────────┤
│  idf.py CLI (subprocess)   │
│  idf.py -C <project> ...   │
└────────────────────────────┘
```

### IDF Environment Discovery (priority order)
1. `--idf-path` CLI argument
2. `IDF_PATH` environment variable
3. `~/esp/esp-idf`
4. `/opt/esp-idf`

The ESP-IDF environment (`export.sh`) is sourced once on first tool use and cached for the process lifetime.

## Key Design Decisions

1. **Subprocess over library**: `idf.py` is the standard ESP-IDF interface. Wrapping it as subprocess ensures identical behavior to manual use and avoids Python library coupling.

2. **Multi-segment flash**: `idf.py flash` handles bootloader + partition table + application automatically. No need to specify addresses or segment layout — the build system knows.

3. **Duration-based monitor**: The `monitor` tool captures for a fixed time then returns, rather than running interactively. This fits the MCP tool model (request → response) and is ideal for boot log capture.

4. **Project discovery via CMakeLists.txt**: Scans for `project()` calls in CMakeLists.txt to distinguish ESP-IDF projects from other CMake projects in the same directory tree.

## Testing

**16 tests** — all pass without ESP-IDF or hardware:

| Category | Count | Description |
|----------|-------|-------------|
| Unit | 5 | Args parsing, config defaults, config from args |
| Tool | 6 | list_targets, list_projects (empty, with projects, missing), build_status (known, unknown) |
| Integration | 5 | Handler creation, config defaults, from args, multiple handlers |

```bash
cd claude-mcps/esp-idf-build && cargo test
```
