# claude-mcps

Model Context Protocol (MCP) servers for extending Claude's capabilities.

## Available MCPs

| MCP | Description |
|-----|-------------|
| [embedded-probe](embedded-probe/) | Embedded debugging and flash programming via probe-rs (27 tools) |
| [zephyr-build](zephyr-build/) | Zephyr RTOS application building via west (5 tools) |
| [esp-idf-build](esp-idf-build/) | ESP-IDF application building, flashing, and monitoring (8 tools) |
| [saleae-logic](saleae-logic/) | Saleae Logic 2 signal capture, protocol decoding, and analysis (18 tools) |

## Structure

```
claude-mcps/
├── embedded-probe/     # Embedded debugging MCP (probe-rs, esptool, nrfjprog)
├── zephyr-build/       # Zephyr build MCP (west build system)
├── esp-idf-build/      # ESP-IDF build MCP (idf.py build system)
└── saleae-logic/       # Saleae Logic 2 MCP (Python, logic2-automation)
```

## Building

Each MCP is self-contained:

```bash
# Build embedded-probe
cd embedded-probe && cargo build --release

# Build zephyr-build
cd zephyr-build && cargo build --release

# Build esp-idf-build
cd esp-idf-build && cargo build --release

# Install saleae-logic (Python)
cd saleae-logic && python3 -m venv .venv && source .venv/bin/activate && pip install -e .
```

## Configuring with Claude Code

Add to your Claude Code MCP settings:

```json
{
  "mcpServers": {
    "embedded-probe": {
      "command": "/path/to/claude-mcps/embedded-probe/target/release/embedded-probe"
    },
    "zephyr-build": {
      "command": "/path/to/claude-mcps/zephyr-build/target/release/zephyr-build",
      "args": ["--workspace", "/path/to/zephyr-workspace"]
    },
    "esp-idf-build": {
      "command": "/path/to/claude-mcps/esp-idf-build/target/release/esp-idf-build",
      "args": ["--projects-dir", "/path/to/esp-dev-kits/examples"]
    },
    "saleae-logic": {
      "command": "/path/to/claude-mcps/saleae-logic/.venv/bin/python",
      "args": ["-m", "saleae_logic"],
      "cwd": "/path/to/claude-mcps/saleae-logic"
    }
  }
}
```

## Related Projects

- [claude-config](../claude-config/) - Claude assistant configuration and skills
