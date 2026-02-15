# Claude MCP Servers

MCP servers for embedded development with Claude. Each server has its own `CLAUDE.md` with detailed tool docs.

## Available MCPs

| Server | Language | Purpose |
|--------|----------|---------|
| `embedded-probe/` | Rust | Debug probes, flash programming, RTT, coredump analysis |
| `zephyr-build/` | Rust | Zephyr RTOS build system (west wrapper) |
| `esp-idf-build/` | Rust | ESP-IDF build, flash, and monitor |
| `saleae-logic/` | Python | Logic analyzer capture and protocol decoding |

## Building

```bash
# Rust servers
cd embedded-probe && cargo build --release
cd zephyr-build && cargo build --release
cd esp-idf-build && cargo build --release

# Python server
cd saleae-logic && pip install -e .
```

## Adding a New MCP Server

1. Create folder: `mkdir my-mcp && cd my-mcp && cargo init`
2. Implement MCP server using rmcp (Rust) or similar
3. Add a `CLAUDE.md` with tool documentation
4. Register in workspace `.mcp.json`
