# PRD: zephyr-build MCP Server

## Purpose

Build server for Zephyr RTOS applications. Wraps the `west` build system so Claude Code can compile firmware for any supported board, manage build artifacts, and run long builds in the background — without generating shell commands for the user to copy-paste.

## Technology Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Language | Rust | Consistent with other MCP servers, fast startup |
| MCP SDK | rmcp 0.3.2 | Official Rust MCP SDK |
| Build Tool | west (subprocess) | Standard Zephyr build system |
| Async | tokio 1 | Background build support |

## Tools (5)

| Tool | Args | Returns |
|------|------|---------|
| `list_apps` | workspace_path | Array of {name, path, has_build, last_board} |
| `list_boards` | filter, include_all | Array of {name, arch, vendor} |
| `build` | app, board, pristine, extra_args, background, workspace_path | {success, output, artifact_path, duration_ms} or {build_id} if background |
| `clean` | app, workspace_path | Clean confirmation |
| `build_status` | build_id | {status, progress, output, artifact_path, error} |

### list_apps

Scans `apps/` directory under the workspace root for directories containing `CMakeLists.txt`. Returns app name, path, whether a build directory exists, and the last board it was built for (from build metadata).

### list_boards

Two modes:
- **Fast** (default): Returns hardcoded list of common boards — nRF52/53, ESP32 variants, STM32, native_sim
- **Full** (`include_all=true`): Runs `west boards` subprocess — comprehensive but slow

Optional `filter` parameter does substring matching (e.g., `filter="nrf"` returns only Nordic boards).

### build

Runs `west build -b <board> apps/<app>`. Options:
- `pristine`: Adds `--pristine` flag for clean rebuild
- `extra_args`: Passed through to west/CMake (e.g., `-DOVERLAY_CONFIG=debug.conf`)
- `background`: Returns immediately with `build_id`; poll with `build_status`

### build_status

Tracks background builds in a `HashMap<build_id, BuildState>`. States: running, complete, failed. Returns accumulated stdout/stderr and artifact path on completion.

## Architecture

```
┌────────────────────────────┐
│  MCP Tool Layer (5 tools)  │
├────────────────────────────┤
│  Background Build Manager  │
│  HashMap<UUID, BuildState> │
│  tokio::spawn per build    │
├────────────────────────────┤
│  west CLI (subprocess)     │
│  west build -b <board> ... │
└────────────────────────────┘
```

### Workspace Detection (priority order)
1. `--workspace` CLI argument
2. `ZEPHYR_WORKSPACE` environment variable
3. Walk up from CWD looking for `.west/` directory
4. Error if not found

## Hardcoded Boards

| Board | Architecture | Vendor |
|-------|-------------|--------|
| nrf52840dk/nrf52840 | ARM | Nordic |
| nrf5340dk/nrf5340/cpuapp | ARM | Nordic |
| esp32_devkitc/esp32/procpu | Xtensa | Espressif |
| esp32s3_eye/esp32s3/procpu | Xtensa | Espressif |
| esp32c3_devkitc | RISC-V | Espressif |
| stm32f4_disco | ARM | ST |
| nucleo_f411re | ARM | ST |
| nucleo_g431rb | ARM | ST |
| native_sim | POSIX | Zephyr |

## Key Design Decisions

1. **Subprocess over library**: No Zephyr or west Python library dependency. Uses the same CLI commands developers would run manually, ensuring identical behavior.

2. **Background builds**: Zephyr builds can take 30-120 seconds. Background mode returns a build_id immediately; Claude can continue the conversation and poll `build_status`.

3. **Hardcoded common boards**: `west boards` takes several seconds to enumerate all boards. The hardcoded list provides instant results for the most common targets. `include_all=true` falls back to `west boards` when needed.

4. **Workspace path flexibility**: Three discovery methods ensure the server works whether launched from the workspace root, a subdirectory, or via Claude Code's MCP configuration.

## Testing

**19 tests** — all pass without west or workspace:

| Category | Count | Description |
|----------|-------|-------------|
| Unit | 5 | Args parsing (defaults, with options), config (defaults, from args) |
| Tool | 9 | list_boards (common, filter, no match), list_apps (dummy, empty, no dir), build_status, clean |
| Integration | 5 | Handler creation, config defaults, from args, multiple handlers |

```bash
cd claude-mcps/zephyr-build && cargo test
```
