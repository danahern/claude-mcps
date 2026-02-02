# claude-mcps

Model Context Protocol (MCP) servers for extending Claude's capabilities.

## Available MCPs

| MCP | Description |
|-----|-------------|
| [embedded-probe](embedded-probe/) | Embedded debugging and flash programming via probe-rs (27 tools) |

## Structure

```
claude-mcps/
├── embedded-probe/     # Embedded debugging MCP (probe-rs, esptool, nrfjprog)
└── <future-mcp>/       # Each MCP is a standalone Rust project
```

## Building

Each MCP is self-contained:

```bash
cd embedded-probe
cargo build --release
```

## Configuring with Claude Code

Add to your Claude Code MCP settings:

```json
{
  "mcpServers": {
    "embedded-probe": {
      "command": "/path/to/claude-mcps/embedded-probe/target/release/embedded-probe"
    }
  }
}
```

## Related Projects

- [claude-config](../claude-config/) - Claude assistant configuration and skills
