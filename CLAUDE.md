# Claude MCP Servers

MCP servers for embedded development with Claude. Each server has its own `CLAUDE.md` with detailed tool docs.

## Available MCPs

| Server | Language | Purpose |
|--------|----------|---------|
| `embedded-probe/` | Rust | Debug probes, flash programming, RTT, coredump analysis |
| `zephyr-build/` | Rust | Zephyr RTOS build system (west wrapper) |
| `elf-analysis/` | Rust | ELF binary size analysis (ROM/RAM breakdown, diffing) |
| `esp-idf-build/` | Rust | ESP-IDF build, flash, and monitor |
| `linux-build/` | Rust | Docker-based Linux cross-compilation and SSH deployment |
| `saleae-logic/` | Python | Logic analyzer capture and protocol decoding |
| `hw-test-runner/` | Python | BLE and TCP hardware testing (WiFi provisioning, throughput) |
| `alif-flash/` | Python | Alif Ensemble (E7/E8) MRAM flash via SE-UART ISP, J-Link, RTT |
| `uart-mcp/` | Python | Bidirectional UART — session-based serial console interaction |

## Building

```bash
# Rust servers
cd embedded-probe && cargo build --release
cd elf-analysis && cargo build --release
cd zephyr-build && cargo build --release
cd esp-idf-build && cargo build --release
cd linux-build && cargo build --release

# Python servers
cd saleae-logic && pip install -e .
cd hw-test-runner && pip install -e .
cd alif-flash && pip install -e .
```

## Adding a New MCP Server

1. Create folder: `mkdir my-mcp && cd my-mcp && cargo init`
2. Implement MCP server using rmcp (Rust) or similar
3. Add a `CLAUDE.md` with tool documentation
4. Register in workspace `.mcp.json`
