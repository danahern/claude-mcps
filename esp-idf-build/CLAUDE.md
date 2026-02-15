# esp-idf-build

ESP-IDF build, flash, and monitor MCP server. Wraps `idf.py` for ESP32 project management.

## Build

```bash
cargo build --release
```

## Configuration

Set `projects_dir` in config or pass per-call. Points to the directory containing ESP-IDF projects.

## Tools

- `list_projects` — Scan for ESP-IDF projects (directories with CMakeLists.txt containing `project()`)
- `list_targets` — List supported ESP32 chips (esp32, esp32s2, esp32s3, esp32c3, esp32c6, esp32p4, etc.)
- `set_target` — Configure project for a chip (runs `idf.py set-target`)
- `build` — Build project (runs `idf.py build`). Supports `background=true`
- `build_status` — Check progress of a background build
- `flash` — Flash all segments (bootloader + partition table + app) via `idf.py flash`
- `monitor` — Capture serial output for a duration via `idf.py monitor`
- `clean` — Remove build directory via `idf.py fullclean`

## Key Details

- Runs `idf.py` as a subprocess with ESP-IDF environment sourced
- `set_target` must be called before first build (configures sdkconfig)
- `flash` handles multi-segment flashing automatically
- `monitor` captures output for `duration_seconds` then returns it (not interactive)
