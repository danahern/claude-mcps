# Claude MCP Servers

This repository contains Model Context Protocol (MCP) servers for use with Claude.

## Available MCPs

### embedded-probe

Embedded debugging and flash programming via probe-rs. 27 tools including:
- Debug probe connection (J-Link, ST-Link, CMSIS-DAP)
- Memory read/write
- Flash programming (erase, program, verify)
- RTT communication
- Boot validation
- Vendor tools (esptool for ESP32, nrfjprog for Nordic)

Build: `cd embedded-probe && cargo build --release`

## Structure

Each MCP is a standalone Rust project in its own folder:
```
claude-mcps/
├── embedded-probe/     # Embedded debugging MCP
└── <future-mcp>/       # Additional MCPs go here
```

## Adding a New MCP Server

1. Create folder: `mkdir my-mcp && cd my-mcp && cargo init`
2. Implement MCP server using rmcp or similar
3. Document in this file
