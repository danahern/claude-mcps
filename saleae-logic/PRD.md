# PRD: saleae-logic MCP Server

## Purpose

Logic analyzer and protocol analysis server for Saleae Logic 2. Provides Claude Code with 21 tools to capture digital and analog signals, decode protocols (I2C, SPI, UART, CAN), perform statistical analysis, read decoded bytes, and generate custom protocol decoders — all through the Logic 2 desktop application's automation API.

## Technology Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Language | Python 3.10+ | Official Saleae SDK is Python-only |
| MCP SDK | mcp >=1.0.0 | Python MCP SDK |
| Logic Automation | logic2-automation >=1.0.6 | Official Saleae gRPC client |
| Data Analysis | numpy >=1.24, pandas >=2.0 | Statistical analysis for deep_analyze |
| Testing | pytest >=7.0, pytest-asyncio | Async test support |

## Tools (21)

### Capture & Device (8)

| Tool | Args | Returns |
|------|------|---------|
| `get_app_info` | — | {app_version, api_version, app_pid} |
| `list_devices` | include_simulation | Array of {device_id, device_type, is_simulation, rate_info} |
| `start_capture` | device_id, channels, analog_channels, sample_rate, analog_sample_rate, duration_seconds, trigger_channel, trigger_type, after_trigger_seconds, logic_level, glitch_filter_ns | {capture_id, mode, channels, rates} |
| `stop_capture` | capture_id | Confirmation |
| `wait_capture` | capture_id | Confirmation |
| `close_capture` | capture_id | Confirmation |
| `save_capture` | capture_id, filepath | File path (absolute) |
| `load_capture` | filepath | {capture_id} |

### Protocol Analysis (4)

| Tool | Args | Returns |
|------|------|---------|
| `add_analyzer` | capture_id, analyzer_name, settings, label | {analyzer_index} |
| `add_high_level_analyzer` | capture_id, extension_directory, name, input_analyzer_index, settings, label | {analyzer_index} |
| `export_analyzer_data` | capture_id, analyzer_index, output_path, radix, max_rows | CSV data inline or file path |
| `export_raw_data` | capture_id, digital_channels, analog_channels, output_dir | {output_dir, files} |

### Intelligence & Analysis (5)

| Tool | Args | Returns |
|------|------|---------|
| `analyze_capture` | capture_id, analyzer_index | Protocol-specific summary: packet counts, errors, addresses, timing |
| `search_protocol_data` | capture_id, analyzer_index, pattern (regex), column, max_results | Matching rows |
| `get_timing_info` | capture_id, channel | {frequency, duty_cycle, pulse_widths} |
| `read_protocol_data` | capture_id, analyzer_index, ascii, max_bytes, radix | {bytes, ascii, protocol, count} |
| `deep_analyze` | capture_id, analyzer_index OR channel OR analog_channel | Statistical analysis (see below) |

### Advanced (4)

| Tool | Args | Returns |
|------|------|---------|
| `configure_trigger` | trigger_channel, trigger_type, min/max_pulse_width, linked_channels, after_trigger_seconds | Trigger configuration stored |
| `compare_captures` | capture_id_a, capture_id_b, analyzer_index_a, analyzer_index_b | {differences, row_counts} |
| `stream_capture` | channels, duration_seconds, analyzer_name, analyzer_settings, device_id, sample_rate, max_rows | Capture + decode + data in single call |
| `create_extension` | name, source OR decode_body, result_types, settings | {extension_directory, class_name} |

## Sample Rate Management

No runtime API exists to query valid sample rates from Logic 2. The server includes a hardcoded rate table per device type and auto-selects rates when not specified.

### Device Rate Table

| Device | Max Digital | Max Analog | Channels | Valid Pairs (digital, analog) |
|--------|-------------|------------|----------|-------------------------------|
| Logic Pro 16 | 500 MS/s | 50 MS/s | 16 | 125M+12.5M, 50M+12.5M, 50M+6.25M, 25M+3.125M |
| Logic Pro 8 | 500 MS/s | 50 MS/s | 8 | 125M+12.5M, 50M+6.25M, 25M+3.125M |
| Logic 8 | 100 MS/s | N/A | 8 | Digital-only |

### Auto-selection
- Analog channels without explicit rates → 50 MS/s digital + 6.25 MS/s analog
- Digital-only without explicit rate → 10 MS/s
- Invalid combinations → error message with suggested valid pairs
- `list_devices` returns per-device `rate_info` with `suggested_pairs`

## Deep Analysis (deep_analyze)

Three analysis modes depending on which argument is provided:

### Protocol Data (analyzer_index)
Uses pandas for CSV analysis:
- Transaction timing: mean, median, std dev, p95, p99
- Inter-transaction gap distribution
- Throughput (transactions/second)
- Error rate percentage
- Protocol-specific: I2C address histogram, UART byte distribution, SPI transfer stats

### Digital Signal (channel)
Uses numpy for raw signal analysis:
- Frequency with jitter percentage
- Duty cycle measurement
- Pulse width distributions (high/low)
- Edge density over time (burst detection)
- Stability score (0-100)

### Analog Signal (analog_channel)
Uses numpy for waveform analysis:
- Basic stats: min, max, mean, RMS, std dev, peak-to-peak
- FFT: dominant frequencies and spectral peaks
- Noise floor estimate
- Crest factor
- Zero-crossing rate

numpy and pandas are lazy-imported — only loaded when `deep_analyze` is called.

## Protocol Byte Extraction (read_protocol_data)

Extracts decoded bytes from protocol analyzers with automatic protocol detection:

| Protocol | Extraction Method | Output |
|----------|------------------|--------|
| UART | Data column values | {bytes, ascii, count} |
| I2C | Address + data bytes, skipping start/stop | {bytes, ascii, count} |
| SPI | Separate MOSI and MISO byte streams | {mosi_bytes, miso_bytes, mosi_ascii, miso_ascii} |

ASCII translation replaces non-printable bytes with `.`, preserves `\n`, `\r`, `\t`.

## Custom HLA Extensions (create_extension)

Generates Logic 2-compatible High Level Analyzer extensions:

### Input Modes
- **`source`**: Full Python HLA class — written as-is
- **`decode_body`**: Just the decode method body — server generates boilerplate class with imports, result_types, settings, and __init__

### Generated Files
```
<output_dir>/extensions/<name_slug>/
├── extension.json          # Logic 2 format: type + entryPoint
└── HighLevelAnalyzer.py    # HLA class with decode() method
```

### extension.json Format (Logic 2 requirement)
```json
{
  "version": "0.0.1",
  "apiVersion": "1.0.0",
  "extensions": {
    "ClassName": {
      "type": "HighLevelAnalyzer",
      "entryPoint": "HighLevelAnalyzer.ClassName"
    }
  }
}
```

### Input Frame Types (from low-level analyzers)
- **I2C**: `type='start'|'stop'|'address'|'data'`, `data.address=[int]`, `data.data=[int]`
- **SPI**: `type='result'`, `data.miso=int`, `data.mosi=int`
- **UART**: `type='data'`, `data.value=int`, `data.parity_error=bool`, `data.framing_error=bool`

## Architecture

```
┌──────────────────────────────────────┐
│  MCP Server (Python)                 │
│  21 tools, UUID-based capture IDs    │
├──────────────────┬───────────────────┤
│  Lazy Imports:   │  State:           │
│  • logic2-auto   │  • captures{}     │
│  • numpy/pandas  │  • analyzers{}    │
│                  │  • next_analyzer  │
└────────┬─────────┴───────────────────┘
         │ gRPC (port 10430)
         ▼
┌──────────────────────────────────────┐
│  Saleae Logic 2 Desktop App         │
│  (separate process, own CWD)        │
└──────────────────────────────────────┘
```

### State Management
- **captures**: `dict[capture_id → Capture]` — 8-char UUID keys
- **analyzers**: `dict[capture_id → dict[analyzer_index → AnalyzerHandle]]`
- **next_analyzer**: Global counter, incremented per analyzer added
- State is in-process only — no persistence across server restarts

### Path Handling
All user-supplied paths are resolved to absolute via `_abs_path()` before sending to Logic 2. Logic 2 is a separate process with its own working directory — relative paths silently fail or write to wrong locations.

Applied to: `save_capture`, `load_capture`, `export_analyzer_data` (output_path).
Config `output_dir` resolved in `Config.__post_init__`.

## Analyzer Settings Reference

| Analyzer | Settings |
|----------|----------|
| I2C | `{"SDA": 0, "SCL": 1}` |
| SPI | `{"MISO": 0, "Clock": 1, "Enable": 2, "MOSI": 3}` |
| Async Serial (UART) | `{"Input Channel": 0, "Bit Rate (Bits/s)": 115200}` |
| CAN | `{"CAN": 0, "Bit Rate": 500000}` |

Settings keys must match Logic 2's internal names exactly (case-sensitive).

## Key Design Decisions

1. **Python over Rust**: The official Saleae SDK (`logic2-automation`) is Python-only. No Rust alternative exists.

2. **All paths absolute**: Logic 2 runs as a separate desktop app with its own CWD. All paths sent over gRPC must be absolute. This was a production bug — `./captures` resolved differently in Logic 2 vs the MCP server.

3. **Lazy imports**: `logic2-automation` imported on first tool use (allows tool listing without Logic 2 running). numpy/pandas imported only by `deep_analyze` (keeps startup under 200ms).

4. **CSV as intermediate format**: Logic 2 exports to CSV via gRPC. All analysis tools (analyze_capture, search_protocol_data, read_protocol_data, deep_analyze) export to temp CSV, process it, then clean up.

5. **Hardcoded rate table**: No API to query valid sample rates at runtime. The table is derived from Saleae specs and validated against real devices.

6. **create_extension generates files, not code in memory**: Logic 2 loads HLAs from disk via extension.json. The tool writes files to `<output_dir>/extensions/<name>/` and returns the path for `add_high_level_analyzer`.

## Bugs Found & Fixed

| Bug | Root Cause | Fix |
|-----|-----------|-----|
| Startup crash | `run_stdio_async()` doesn't exist in mcp SDK | Use `stdio_server` context manager with `Server.run()` |
| Relative path failures | Logic 2 resolves paths from its own CWD, not MCP server's | `_abs_path()` helper + `Config.__post_init__` |
| Invalid sample rate on first capture | No auto-selection, raw rates passed through | Auto-select 50M/6.25M for analog captures |
| HLA extension load failure | extension.json used `entry` instead of `type` + `entryPoint` | Fixed to Logic 2's required format |

## Testing

**25 tests** — all pass without Logic 2 or hardware:

| Category | Count | Location | Description |
|----------|-------|----------|-------------|
| Unit | 15 | test_analysis.py | CSV parsing, I2C/SPI/UART analysis, timing computation, search |
| Smoke | 10 | test_server_startup.py | Server creation, handler registration, tool count (21), config paths |

Hardware-dependent tests (require Logic 2 running):
- test_connection.py — get_app_info, list_devices
- test_capture.py — capture lifecycle
- test_analyzers.py — protocol decoders, export

```bash
cd claude-mcps/saleae-logic
.venv/bin/python -m pytest tests/test_analysis.py tests/test_server_startup.py -v
```

## Prerequisites

- Saleae Logic 2 v2.3.56+ installed and running
- Automation enabled in Logic 2 Preferences (default port 10430)
- A Saleae analyzer (physical or simulation device)
