# saleae-logic

Logic analyzer MCP server for Saleae Logic 2. Capture signals, decode protocols, and analyze data.

## Setup

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -e .
```

Requires Saleae Logic 2 (v2.3.56+) with automation server enabled (Preferences > Enable automation, port 10430).

## Tools by Category

### Device & Capture
- `get_app_info` — Check Logic 2 connection status
- `list_devices` — Find connected analyzers
- `start_capture` — Record signals (timed, triggered, or manual modes)
- `stop_capture` — Stop a manual capture
- `wait_capture` — Wait for timed/triggered capture to complete
- `save_capture` / `load_capture` — .sal file I/O
- `close_capture` — Release capture resources

### Protocol Decoding
- `add_analyzer` — Add protocol decoder (I2C, SPI, UART, CAN, etc.)
- `add_high_level_analyzer` — Attach custom HLA extension
- `create_extension` — Generate HLA Python extension from a decode spec
- `export_analyzer_data` — Export decoded data as CSV
- `read_protocol_data` — Read decoded bytes (hex + ASCII)

### Analysis
- `analyze_capture` — Smart summary: packet counts, errors, timing, addresses
- `search_protocol_data` — Search decoded data by pattern/regex
- `get_timing_info` — Frequency, duty cycle, pulse widths from raw digital
- `deep_analyze` — Statistical analysis (jitter, FFT, throughput, error rates)
- `compare_captures` — Diff two captures for regression testing

### Convenience
- `stream_capture` — One-shot: capture + decode + return results
- `configure_trigger` — Set up trigger with linked channel conditions

## Key Details

- Python server using the Saleae Logic 2 automation API
- Supports both real hardware and simulation devices
- Analyzer settings are protocol-specific (e.g., I2C: `{"SCL": 0, "SDA": 1}`, UART: `{"Input Channel": 0, "Bit Rate": 115200}`)
- `stream_capture` is the fastest path for simple capture-and-decode workflows
