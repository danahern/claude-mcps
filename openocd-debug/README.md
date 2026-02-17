# openocd-debug

MCP server for embedded debugging and firmware loading via OpenOCD. Communicates with OpenOCD's TCL socket interface (port 6666) to control targets, load firmware, read/write memory, and capture UART output.

Designed for dual-core SoCs like STM32MP1 where the M4 core lacks persistent flash — firmware is loaded to RAM and lost on power cycle.

## Prerequisites

Install OpenOCD:

```bash
brew install open-ocd
```

If upstream OpenOCD doesn't support your target, ST maintains a fork at `github.com/STMicroelectronics/OpenOCD`.

## Building

```bash
cd claude-mcps/openocd-debug
cargo build --release
```

Binary: `target/release/openocd-debug`

## Configuration

```bash
openocd-debug [OPTIONS]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--openocd-path` | Search PATH | Path to openocd binary |
| `--serial-port` | None | Default serial port for `monitor` tool |
| `--log-level` | `info` | Log level (error, warn, info, debug, trace) |
| `--log-file` | stderr | Log file path |

### MCP Registration

Add to `.mcp.json`:

```json
{
  "mcpServers": {
    "openocd-debug": {
      "command": "/path/to/openocd-debug",
      "args": ["--serial-port", "/dev/ttyACM0"]
    }
  }
}
```

## Tools

### Session Management

| Tool | Description |
|------|-------------|
| `connect(cfg_file, extra_args?)` | Start OpenOCD daemon, return session_id |
| `disconnect(session_id)` | Stop OpenOCD and release session |

### Target Control

| Tool | Description |
|------|-------------|
| `get_status(session_id)` | Target state and PC register |
| `halt(session_id)` | Halt CPU execution |
| `run(session_id)` | Resume CPU execution |
| `reset(session_id, halt_after_reset?)` | Reset target (halt or run) |

### Firmware Loading

| Tool | Description |
|------|-------------|
| `load_firmware(session_id, file_path, address?)` | Load ELF/HEX/BIN to target memory |

BIN files require an `address` parameter (e.g., `"0x10000000"` for MCUSRAM). ELF and HEX files contain their own address information.

### Memory Operations

| Tool | Description |
|------|-------------|
| `read_memory(session_id, address, count?, format?)` | Read 32-bit words from target |
| `write_memory(session_id, address, value)` | Write a 32-bit word to target |

### Serial Monitor

| Tool | Description |
|------|-------------|
| `monitor(session_id, port?, baud_rate?, duration_seconds?)` | Capture UART output for a duration |

## Typical Workflow

```
# 1. Connect to M4 core via OpenOCD
openocd-debug.connect(cfg_file="board/stm32mp15_dk2.cfg")

# 2. Load firmware to M4 RAM
openocd-debug.load_firmware(session_id, file_path="/path/to/zephyr.elf")

# 3. Reset and run
openocd-debug.reset(session_id, halt_after_reset=false)

# 4. Capture UART console output
openocd-debug.monitor(session_id, port="/dev/ttyACM0", duration_seconds=5)

# 5. Disconnect
openocd-debug.disconnect(session_id)
```

## STM32MP1 Notes

- **M4 has no persistent flash.** Firmware is loaded to RETRAM (64KB @ 0x00000000) or MCUSRAM (384KB @ 0x10000000) via `load_image`. Power cycling requires reloading.
- **Port allocation.** Each session gets 3 consecutive ports (TCL, GDB, telnet) starting at 6666. Multiple sessions can coexist.
- **OpenOCD config.** Use `board/stm32mp15_dk2.cfg` for STM32MP157 DK boards. The config targets the M4 core by default.

## Testing

```bash
cargo test
```

16 tests — all pass without OpenOCD or hardware:
- Config: 3 tests (PATH search, bad path, defaults)
- TCL client: 9 tests (address parsing, memory dump parsing, terminator)
- Main: 4 tests (arg parsing, config creation)
