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
| `--output-dir` | `./captures` | Default directory for exports (resolved to absolute) |
| `--log-level` | `info` | Logging level |

## Tools (19)

### Core Capture & Device

| Tool | Description |
|------|-------------|
| `get_app_info` | Get Logic 2 version and connection status |
| `list_devices` | List connected analyzers with rate info and suggested sample rate pairs |
| `start_capture` | Start recording (digital + analog, manual/timed/triggered, auto rate selection) |
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
| `deep_analyze` | Statistical analysis with numpy/pandas (timing distributions, FFT, jitter, error rates) |

### Advanced

| Tool | Description |
|------|-------------|
| `configure_trigger` | Set up digital triggers with linked channels |
| `compare_captures` | Diff two captures for regression testing |
| `stream_capture` | One-shot: capture + decode + return results |

## Sample Rates

Sample rates are device-dependent and must be valid pairs when using both digital and analog channels. The server handles this automatically:

- **Auto-selection**: Omit `sample_rate` and `analog_sample_rate` — the server picks 50M/6.25M for analog captures, 10M for digital-only
- **Rate info**: `list_devices()` returns `suggested_pairs` per device
- **Helpful errors**: Invalid rate combinations return suggested valid pairs

### Device Specifications

| Device | Max Digital | Max Analog | Channels |
|--------|-------------|------------|----------|
| Logic Pro 16 | 500 MS/s | 50 MS/s | 16 |
| Logic Pro 8 | 500 MS/s | 50 MS/s | 8 |
| Logic 8 | 100 MS/s | — | 8 |

### Common Valid Pairs (digital / analog)

| Pair | Use Case |
|------|----------|
| 125M / 12.5M | High-speed protocols |
| 50M / 12.5M | Balanced (default recommendation) |
| 50M / 6.25M | Auto-selected default |
| 25M / 3.125M | Many channels, lower bandwidth |

## Deep Analysis

The `deep_analyze` tool provides statistical analysis beyond basic counting:

**Protocol data** (`analyzer_index`):
- Transaction timing: mean, median, std dev, p95, p99
- Inter-transaction gap distribution
- Throughput (transactions/sec)
- Error rate percentage
- Protocol-specific: I2C address histogram, UART byte distribution, SPI transfer stats

**Digital signals** (`channel`):
- Frequency statistics with jitter percentage
- Duty cycle measurement
- Pulse width distributions (high/low)
- Edge density over time (burst detection)
- Stability score (0-100)

**Analog signals** (`analog_channel`):
- Basic stats: min, max, mean, RMS, std dev, peak-to-peak
- FFT: dominant frequencies and spectral peaks
- Noise floor estimate
- Crest factor, zero-crossing rate

## Example Workflows

### Debug I2C Sensor Communication

```
1. list_devices()
2. start_capture(channels=[0,1], duration_seconds=2)
3. wait_capture(capture_id)
4. add_analyzer(capture_id, "I2C", {"SCL": 0, "SDA": 1})
5. analyze_capture(capture_id, analyzer_index)
6. deep_analyze(capture_id, analyzer_index=0)
```

### Capture 8 Digital + 8 Analog Channels

```
1. list_devices(include_simulation=True)  # check rate_info
2. start_capture(device_id="...", channels=[0..7], analog_channels=[0..7], duration_seconds=2)
   # rates auto-selected: 50M digital / 6.25M analog
3. wait_capture(capture_id)
4. deep_analyze(capture_id, channel=0)          # digital signal stats
5. deep_analyze(capture_id, analog_channel=0)   # analog FFT + stats
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
# Unit + smoke tests (no hardware needed)
.venv/bin/python -m pytest tests/test_analysis.py tests/test_server_startup.py -v

# Integration tests (requires Logic 2 running)
.venv/bin/python -m pytest tests/test_connection.py tests/test_capture.py tests/test_analyzers.py -v
```

## Troubleshooting

- **Connection refused**: Ensure Logic 2 is running and automation server is enabled in Preferences
- **No devices**: Connect a Saleae analyzer or use `list_devices(include_simulation=True)`
- **Analyzer settings error**: Check exact setting keys by exporting a `.logic2Preset` from the Logic 2 UI
- **Invalid sample rate**: Use `list_devices()` to see `suggested_pairs` for your device, or omit rates for auto-selection
- **Path errors on export**: Paths are now resolved to absolute automatically; if you see relative path errors, restart the MCP server
