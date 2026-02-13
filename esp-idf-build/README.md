# ESP-IDF Build MCP Server

MCP server for building ESP-IDF applications. Wraps `idf.py` to enable AI-assisted firmware building, flashing, and monitoring.

## Tools (8)

| Tool | Description |
|------|-------------|
| `list_projects` | Scan projects directory for ESP-IDF projects |
| `list_targets` | List supported ESP32 target chips |
| `set_target` | Set project's target chip (idf.py set-target) |
| `build` | Build a project (supports background builds) |
| `flash` | Flash all segments (bootloader + partition table + app) |
| `clean` | Remove build artifacts (idf.py fullclean) |
| `build_status` | Check status of background builds |
| `monitor` | Capture serial output with timeout |

## Quick Start

```bash
# Build
cargo build --release

# Run (with projects directory)
./target/release/esp-idf-build --projects-dir /path/to/projects
```

## Configuration

The server finds ESP-IDF via (in order):
1. `--idf-path` CLI argument
2. `IDF_PATH` environment variable
3. `~/esp/esp-idf`
4. `/opt/esp-idf`

The IDF environment (`export.sh`) is sourced once on first tool use and cached.

## Example Usage

```json
// List supported targets
{"method": "tools/call", "params": {"name": "list_targets"}}

// List projects
{"method": "tools/call", "params": {"name": "list_projects", "arguments": {
  "projects_dir": "/path/to/esp-dev-kits/examples"
}}}

// Set target chip
{"method": "tools/call", "params": {"name": "set_target", "arguments": {
  "project": "esp32-p4-eye/factory",
  "target": "esp32p4"
}}}

// Build
{"method": "tools/call", "params": {"name": "build", "arguments": {
  "project": "esp32-p4-eye/factory"
}}}

// Build in background
{"method": "tools/call", "params": {"name": "build", "arguments": {
  "project": "esp32-p4-eye/factory",
  "background": true
}}}

// Check build status
{"method": "tools/call", "params": {"name": "build_status", "arguments": {"build_id": "abc123"}}}

// Flash
{"method": "tools/call", "params": {"name": "flash", "arguments": {
  "project": "esp32-p4-eye/factory",
  "port": "/dev/cu.usbserial-1110"
}}}

// Monitor serial output (10 seconds)
{"method": "tools/call", "params": {"name": "monitor", "arguments": {
  "project": "esp32-p4-eye/factory",
  "port": "/dev/cu.usbserial-1110",
  "duration_seconds": 10
}}}

// Clean
{"method": "tools/call", "params": {"name": "clean", "arguments": {"project": "esp32-p4-eye/factory"}}}
```

## Requirements

- [ESP-IDF](https://docs.espressif.com/projects/esp-idf/en/latest/) (v5.x)
- Python 3.8+

## Supported Targets

| Target | Architecture | Description |
|--------|-------------|-------------|
| esp32 | xtensa | Dual-core Xtensa LX6, WiFi + BLE |
| esp32s2 | xtensa | Single-core Xtensa LX7, WiFi |
| esp32s3 | xtensa | Dual-core Xtensa LX7, WiFi + BLE |
| esp32c2 | riscv | Single-core RISC-V, WiFi + BLE |
| esp32c3 | riscv | Single-core RISC-V, WiFi + BLE |
| esp32c5 | riscv | Single-core RISC-V, WiFi 6 + BLE |
| esp32c6 | riscv | Single-core RISC-V, WiFi 6 + BLE + 802.15.4 |
| esp32c61 | riscv | Single-core RISC-V, WiFi 6 + BLE |
| esp32h2 | riscv | Single-core RISC-V, BLE + 802.15.4 |
| esp32p4 | riscv | Dual-core RISC-V, high-performance |
