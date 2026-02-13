# saleae-logic MCP Server

MCP server for controlling Saleae Logic 2 analyzers — capture signals, decode protocols, and analyze data directly from Claude.

## Prerequisites

- **Saleae Logic 2** (v2.3.56+) with automation server enabled
  - Preferences → Enable automation server (default port: 10430)
- **Python 3.10+**
- A Saleae Logic analyzer (or use simulation devices)

## Setup

```bash
cd claude-mcps/saleae-logic
python3 -m venv .venv
source .venv/bin/activate
pip install -e .
```

## Claude Code MCP Configuration

Add to your Claude Code MCP settings:

```json
{
  "mcpServers": {
    "saleae-logic": {
      "command": "/path/to/claude-mcps/saleae-logic/.venv/bin/python",
      "args": ["-m", "saleae_logic"],
      "cwd": "/path/to/claude-mcps/saleae-logic"
    }
  }
}
```

### CLI Arguments

| Argument | Default | Description |
|----------|---------|-------------|
| `--host` | `127.0.0.1` | Logic 2 automation host |
| `--port` | `10430` | Logic 2 automation port |
| `--output-dir` | `./captures` | Default directory for exports |
| `--log-level` | `info` | Logging level |

## Tools (18)

### Core Capture & Device

| Tool | Description |
|------|-------------|
| `get_app_info` | Get Logic 2 version and connection status |
| `list_devices` | List connected Saleae analyzers |
| `start_capture` | Start recording (digital + analog, manual/timed/triggered) |
| `stop_capture` | Stop an active manual capture |
| `wait_capture` | Wait for a timed/triggered capture to complete |
| `close_capture` | Close and release a capture |
| `save_capture` | Save capture to .sal file |
| `load_capture` | Load a previously saved .sal capture |

### Protocol Analysis

| Tool | Description |
|------|-------------|
| `add_analyzer` | Add protocol decoder (SPI, I2C, UART, CAN, etc.) |
| `add_high_level_analyzer` | Attach custom HLA extension |
| `export_analyzer_data` | Export decoded protocol data as CSV |
| `export_raw_data` | Export raw channel data as CSV |

### Intelligence & Analysis

| Tool | Description |
|------|-------------|
| `analyze_capture` | Smart summary: packet counts, errors, addresses, timing |
| `search_protocol_data` | Search analyzer results for specific values/patterns |
| `get_timing_info` | Calculate frequency, duty cycle, pulse widths |

### Advanced

| Tool | Description |
|------|-------------|
| `configure_trigger` | Set up digital triggers with linked channels |
| `compare_captures` | Diff two captures for regression testing |
| `stream_capture` | One-shot: capture + decode + return results |

## Example Workflows

### Debug I2C Sensor Communication

```
1. list_devices()
2. start_capture(channels=[0,1], duration_seconds=2)
3. wait_capture(capture_id)
4. add_analyzer(capture_id, "I2C", {"SCL": 0, "SDA": 1})
5. export_analyzer_data(capture_id, analyzer_index)
6. analyze_capture(capture_id, analyzer_index)
```

### Verify UART Boot Output

```
1. start_capture(channels=[0], duration_seconds=5)
2. wait_capture(capture_id)
3. add_analyzer(capture_id, "Async Serial", {"Input Channel": 0, "Bit Rate": 115200})
4. export_analyzer_data(capture_id, analyzer_index)
```

### Quick SPI Capture (stream_capture)

```
stream_capture(
    channels=[0,1,2,3],
    duration_seconds=1,
    analyzer_name="SPI",
    analyzer_settings={"MISO": 0, "Clock": 1, "Enable": 2}
)
```

## Analyzer Settings Reference

Settings keys must match what Logic 2 uses. Common configurations:

| Analyzer | Settings |
|----------|----------|
| I2C | `{"SDA": 0, "SCL": 1}` |
| SPI | `{"MISO": 0, "Clock": 1, "Enable": 2}` |
| Async Serial | `{"Input Channel": 0, "Bit Rate": 115200}` |
| CAN | `{"CAN": 0, "Bit Rate": 500000}` |

Tip: Configure the analyzer in Logic 2 UI, export a `.logic2Preset` file, and inspect the JSON for exact keys.

## Testing

```bash
# Unit tests (no hardware needed)
pytest tests/test_analysis.py -v

# Integration tests (requires Logic 2 running)
pytest tests/test_connection.py -v
pytest tests/test_capture.py -v
pytest tests/test_analyzers.py -v
```

## Troubleshooting

- **Connection refused**: Ensure Logic 2 is running and automation server is enabled in Preferences
- **No devices**: Connect a Saleae analyzer or use `list_devices(include_simulation=True)`
- **Analyzer settings error**: Check exact setting keys by exporting a `.logic2Preset` from the Logic 2 UI
