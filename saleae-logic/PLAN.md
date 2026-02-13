# saleae-logic MCP Server — Implementation Plan

## Status: In Progress — Two bugs fixed, awaiting hardware re-test

All 18 tools implemented, unit tests passing (15/15). Two runtime bugs fixed (startup crash, raw export path). Validated with Logic 2 simulation device: capture + wait works, raw export now uses absolute paths.

## Architecture

```
claude-mcps/saleae-logic/
├── PLAN.md
├── pyproject.toml
├── README.md
├── tests/
│   ├── test_connection.py   # get_app_info, list_devices (needs Logic 2)
│   ├── test_capture.py      # capture lifecycle (needs Logic 2)
│   ├── test_analyzers.py    # add_analyzer, export (needs Logic 2)
│   └── test_analysis.py     # CSV parsing, analysis (unit tests, no hardware)
└── src/
    └── saleae_logic/
        ├── __init__.py
        ├── __main__.py      # Entry point
        ├── server.py        # MCP server + 18 tool definitions
        ├── config.py        # CLI args
        └── analysis.py      # CSV parsing and data analysis
```

## Tools (18)

### Core (8)
1. get_app_info
2. list_devices
3. start_capture (manual/timed/triggered, digital+analog)
4. stop_capture
5. wait_capture
6. close_capture
7. save_capture
8. load_capture

### Protocol Analysis (4)
9. add_analyzer
10. add_high_level_analyzer
11. export_analyzer_data
12. export_raw_data

### Intelligence (3)
13. analyze_capture
14. search_protocol_data
15. get_timing_info

### Advanced (3)
16. configure_trigger
17. compare_captures
18. stream_capture

## Key Decisions

- **Python** over Rust: official SDK (`logic2-automation`), data analysis needs
- **Single server.py**: all tools in one file, analysis helpers split to `analysis.py`
- **Lazy SDK import**: `logic2-automation` imported on first use, allowing tests without Logic 2
- **Capture tracking**: UUID-based capture_id, global analyzer index counter

## Bugs Found & Fixed

### Startup crash: `run_stdio_async()` does not exist (fixed 2025-02)
- **Symptom**: `AttributeError: 'Server' object has no attribute 'run_stdio_async'`
- **Root cause**: `__main__.py` called `server.run_stdio_async()` but the low-level `mcp.server.Server` class doesn't have that method
- **Fix**: Use `mcp.server.stdio.stdio_server` context manager with `Server.run()`:
  ```python
  from mcp.server.stdio import stdio_server
  async with stdio_server() as (read_stream, write_stream):
      await server.run(read_stream, write_stream, init_options)
  ```
- **MCP SDK version**: 1.26.0 (Python `mcp` package)
- **Lesson**: The `mcp` Python package has two server APIs — `FastMCP` (high-level, has `run()` with built-in stdio) and `Server` (low-level, requires explicit transport setup). We use the low-level `Server` with `@server.list_tools()` / `@server.call_tool()` decorators, which requires manual stdio wiring.

### Raw data export produces empty directories (fixed 2026-02)
- **Symptom**: `get_timing_info` and `export_raw_data` return "No raw data exported" — directories are created but contain no files
- **Root cause**: `config.output_dir` defaults to `./captures` (relative). The Python process creates directories relative to its own CWD, but `export_raw_data_csv` sends the path over gRPC to the Logic 2 app, which resolves it relative to Logic 2's CWD — a different directory.
- **Fix**: Convert `output_dir` to absolute path in `config.py` via `os.path.abspath()`, and also in `_handle_export_raw_data` for user-provided overrides.
- **Lesson**: Any path sent to the Logic 2 automation API must be absolute because Logic 2 is a separate process with its own working directory.

### Test coverage gap: unit tests don't exercise server startup (fixed 2026-02)
- **Observation**: Originally 15 tests all in `test_analysis.py` — pure CSV parsing, never exercising server startup. Hardware test files (`test_connection.py`, etc.) had broken async calls (`await server.call_tool()` doesn't exist on low-level `Server`).
- **Fix**: Added 10 smoke tests to `test_server_startup.py` covering server creation, handler registration, tool count, and config paths. Fixed hardware test files to use `server.request_handlers[CallToolRequest]` pattern.

## Testing

### Running Tests

```bash
cd claude-mcps/saleae-logic && .venv/bin/python -m pytest tests/test_analysis.py tests/test_server_startup.py -v
```

The tests above run without Logic 2 or hardware. The other test files (`test_connection.py`, `test_capture.py`, `test_analyzers.py`) require Logic 2 running with automation server enabled.

### Test Coverage (25 tests)

**Unit tests** (`tests/test_analysis.py` — 15 tests):
- CSV parsing and data analysis functions
- Pure logic tests, no server or hardware needed

**Smoke tests** (`tests/test_server_startup.py` — 10 tests):
- `test_create_server_returns_valid_server` — Server has `run()` and `create_initialization_options()`
- `test_create_initialization_options` — Returns without error
- `test_server_registers_tool_handlers` — ListTools and CallTool handlers registered
- `test_stdio_server_import` — `mcp.server.stdio.stdio_server` is importable
- `test_parse_args_output_dir_is_absolute` — Default output_dir is absolute
- `test_parse_args_relative_output_dir_becomes_absolute` — Relative paths converted to absolute
- `test_parse_args_absolute_output_dir_unchanged` — Absolute paths pass through
- `test_tool_count` — Exactly 18 tools registered
- `test_all_tool_names` — All 18 expected tool names present
- `test_unknown_tool_returns_error` — Unknown tool returns error, not crash

**Hardware tests** (`tests/test_connection.py` — 3 tests, requires Logic 2):
- `test_get_app_info` — Version info from Logic 2
- `test_list_devices` — Device list (with simulation)

**Hardware tests** (`tests/test_capture.py` — 3 tests, requires Logic 2):
- `test_timed_capture_lifecycle` — Start → wait → save → close
- `test_manual_capture_lifecycle` — Start → stop → close
- `test_load_capture` — Save then load .sal file

**Hardware tests** (`tests/test_analyzers.py` — 3 tests, requires Logic 2):
- `test_add_analyzer` — Add I2C analyzer to capture
- `test_export_analyzer_data` — Export decoded protocol data
- `test_export_raw_data` — Export raw channel data

### Key Testing Pattern

The MCP Python SDK's low-level `Server` registers handlers via decorators (`@server.list_tools()`, `@server.call_tool()`). To invoke tools in tests, use the `request_handlers` dict directly:

```python
from mcp.types import CallToolRequest

req = CallToolRequest(
    method="tools/call",
    params={"name": tool_name, "arguments": args},
)
result = await server.request_handlers[CallToolRequest](req)
content = result.root.content  # list of TextContent
```

## Remaining Work

- [ ] Re-test `get_timing_info` and `export_raw_data` after absolute path fix (requires restart)
- [ ] Duty cycle measurement on D0/D1 with simulation device
- [ ] Test protocol decoders (I2C, SPI, UART) with real signals
