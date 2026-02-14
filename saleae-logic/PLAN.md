# saleae-logic MCP Server — Implementation Plan

## Status: 20 tools — path fix, smart rates, deep analysis, protocol byte reader

All 20 tools implemented, 25 unit/smoke tests passing. Four enhancements:
1. **Path fix**: All user-supplied paths resolved to absolute before sending to Logic 2
2. **Smart sample rates**: Auto-selection for analog captures, rate table per device, helpful error messages
3. **Deep analysis**: New `deep_analyze` tool with numpy/pandas statistical analysis
4. **Protocol byte reader**: New `read_protocol_data` tool extracts decoded bytes from UART/I2C/SPI with ASCII translation

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
│   ├── test_analysis.py     # CSV parsing, analysis (unit tests, no hardware)
│   └── test_server_startup.py # Server creation, tool registration (no hardware)
└── src/
    └── saleae_logic/
        ├── __init__.py
        ├── __main__.py      # Entry point
        ├── server.py        # MCP server + 19 tool definitions
        ├── config.py        # CLI args
        └── analysis.py      # CSV parsing, basic + deep analysis
```

## Tools (20)

### Core (8)
1. get_app_info
2. list_devices — now includes rate_info per device
3. start_capture (manual/timed/triggered, digital+analog, auto rate selection)
4. stop_capture
5. wait_capture
6. close_capture
7. save_capture — path resolved to absolute
8. load_capture — path resolved to absolute

### Protocol Analysis (4)
9. add_analyzer
10. add_high_level_analyzer
11. export_analyzer_data — user-supplied output_path resolved to absolute
12. export_raw_data

### Intelligence (5)
13. analyze_capture
14. search_protocol_data
15. get_timing_info
16. **read_protocol_data** — NEW: extract decoded bytes with ASCII translation
17. **deep_analyze** — NEW: statistical analysis with numpy/pandas

### Advanced (3)
18. configure_trigger
19. compare_captures
20. stream_capture

## Key Decisions

- **Python** over Rust: official SDK (`logic2-automation`), data analysis needs
- **Single server.py**: all tools in one file, analysis helpers split to `analysis.py`
- **Lazy SDK import**: `logic2-automation` imported on first use, allowing tests without Logic 2
- **Lazy numpy/pandas import**: only loaded when `deep_analyze` is called
- **Capture tracking**: UUID-based capture_id, global analyzer index counter
- **Rate table hardcoded**: No runtime API to query valid rates from Logic 2

## Bugs Found & Fixed

### Startup crash: `run_stdio_async()` does not exist (fixed 2025-02)
- **Symptom**: `AttributeError: 'Server' object has no attribute 'run_stdio_async'`
- **Root cause**: `__main__.py` called `server.run_stdio_async()` but the low-level `mcp.server.Server` class doesn't have that method
- **Fix**: Use `mcp.server.stdio.stdio_server` context manager with `Server.run()`
- **Lesson**: The `mcp` Python package has two server APIs — `FastMCP` (high-level) and `Server` (low-level). We use the low-level `Server` with `@server.list_tools()` / `@server.call_tool()` decorators.

### Relative paths cause "No such file or directory" (fixed 2026-02)
- **Symptom**: `analyze_capture`, `get_timing_info`, `save_capture`, `load_capture`, `export_analyzer_data` fail with path errors
- **Root cause**: Paths sent over gRPC to Logic 2 (a separate process) are resolved relative to Logic 2's CWD, not the MCP server's CWD
- **Fix**:
  1. `Config.__post_init__` converts `output_dir` to absolute
  2. `_abs_path()` helper resolves user-supplied paths in `save_capture`, `load_capture`, `export_analyzer_data`
- **Lesson**: Any path sent to the Logic 2 automation API must be absolute.

## Testing

### Running Tests

```bash
cd claude-mcps/saleae-logic && .venv/bin/python -m pytest tests/test_analysis.py tests/test_server_startup.py -v
```

### Test Coverage (25 tests)

**Unit tests** (`tests/test_analysis.py` — 15 tests):
- CSV parsing and data analysis functions
- Pure logic tests, no server or hardware needed

**Smoke tests** (`tests/test_server_startup.py` — 10 tests):
- Server creation, handler registration, tool count (20), config paths

**Hardware tests** (require Logic 2):
- `test_connection.py` — get_app_info, list_devices
- `test_capture.py` — capture lifecycle
- `test_analyzers.py` — protocol decoders, export

## Remaining Work

- [ ] End-to-end test after MCP server restart (path fixes, auto rates, deep_analyze)
- [ ] Test deep_analyze with real protocol data from simulation device
- [ ] Test protocol decoders (I2C, SPI, UART) with real signals
