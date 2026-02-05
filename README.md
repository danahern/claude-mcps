# claude-mcps

Model Context Protocol (MCP) servers for extending Claude's capabilities.

## Available MCPs

| MCP | Description |
|-----|-------------|
| [embedded-probe](embedded-probe/) | Embedded debugging and flash programming via probe-rs (27 tools) |
| [zephyr-build](zephyr-build/) | Zephyr RTOS application building via west (5 tools) |

## Structure

```
claude-mcps/
├── embedded-probe/     # Embedded debugging MCP (probe-rs, esptool, nrfjprog)
├── zephyr-build/       # Zephyr build MCP (west build system)
└── <future-mcp>/       # Each MCP is a standalone Rust project
```

## Building

Each MCP is self-contained:

```bash
# Build embedded-probe
cd embedded-probe && cargo build --release

# Build zephyr-build
cd zephyr-build && cargo build --release
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
    }
  }
}
```

## Related Projects

- [claude-config](../claude-config/) - Claude assistant configuration and skills
