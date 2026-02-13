# saleae-logic MCP Server — Implementation Plan

## Status: Complete

All 18 tools implemented, unit tests passing (15/15).

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
