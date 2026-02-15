# Zephyr Build MCP Server

MCP server for building Zephyr RTOS applications. Wraps the `west` build system to enable AI-assisted firmware building.

## Tools (8)

| Tool | Description |
|------|-------------|
| `list_apps` | List available Zephyr applications in the workspace |
| `list_boards` | List supported target boards |
| `build` | Build an application for a target board |
| `clean` | Remove build artifacts |
| `build_status` | Check status of background builds |
| `run_tests` | Run Zephyr tests using twister with parsed results |
| `test_status` | Check status of a background test run |
| `test_results` | Parse results from a completed test run |

## Quick Start

```bash
# Build
cargo build --release

# Run
./target/release/zephyr-build --workspace /path/to/workspace
```

## Configuration

The server finds the Zephyr workspace via (in order):
1. `--workspace` CLI argument
2. `ZEPHYR_WORKSPACE` environment variable
3. Searching for `.west/` in current or parent directories

## Example Usage

```json
// List available apps
{"method": "tools/call", "params": {"name": "list_apps"}}

// List boards (filtered)
{"method": "tools/call", "params": {"name": "list_boards", "arguments": {"filter": "nrf"}}}

// Build an application
{"method": "tools/call", "params": {"name": "build", "arguments": {
  "app": "ble_wifi_bridge",
  "board": "nrf52840dk/nrf52840",
  "pristine": true
}}}

// Build in background
{"method": "tools/call", "params": {"name": "build", "arguments": {
  "app": "ble_wifi_bridge",
  "board": "esp32_devkitc/esp32/procpu",
  "background": true
}}}

// Check build status
{"method": "tools/call", "params": {"name": "build_status", "arguments": {"build_id": "abc123"}}}

// Clean build artifacts
{"method": "tools/call", "params": {"name": "clean", "arguments": {"app": "ble_wifi_bridge"}}}

// Run tests
{"method": "tools/call", "params": {"name": "run_tests", "arguments": {
  "board": "qemu_cortex_m3"
}}}

// Run tests for a specific library
{"method": "tools/call", "params": {"name": "run_tests", "arguments": {
  "path": "lib/crash_log",
  "board": "qemu_cortex_m3"
}}}

// Run tests in background
{"method": "tools/call", "params": {"name": "run_tests", "arguments": {
  "board": "qemu_cortex_m3",
  "background": true
}}}

// Check test status
{"method": "tools/call", "params": {"name": "test_status", "arguments": {"test_id": "abc123"}}}

// Get parsed test results
{"method": "tools/call", "params": {"name": "test_results", "arguments": {"test_id": "abc123"}}}
```

## Requirements

- [West](https://docs.zephyrproject.org/latest/develop/west/index.html) (Zephyr meta-tool)
- Initialized Zephyr workspace with `west init` and `west update`

## Supported Boards (Common)

| Board | Architecture | Vendor |
|-------|-------------|--------|
| nrf52840dk/nrf52840 | arm | Nordic |
| nrf5340dk/nrf5340/cpuapp | arm | Nordic |
| esp32_devkitc/esp32/procpu | xtensa | Espressif |
| esp32s3_eye/esp32s3/procpu | xtensa | Espressif |
| esp32c3_devkitc | riscv | Espressif |
| stm32f4_disco | arm | ST |
| native_sim | posix | Zephyr |

Use `list_boards` with `include_all: true` for complete board list from west.
