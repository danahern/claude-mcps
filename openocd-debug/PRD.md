# PRD: openocd-debug MCP Server

## Purpose

Debug and firmware loading server for targets accessible via OpenOCD. Wraps OpenOCD's TCL socket interface so Claude Code can connect to targets, load firmware, inspect memory, and capture serial output — without the developer managing OpenOCD processes or knowing TCL commands.

Primary use case: STM32MP1 Cortex-M4 core, where firmware lives in RAM (no persistent flash) and must be loaded via `load_image` on every power cycle.

## Technology Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Language | Rust | Consistent with other MCP servers, fast startup |
| MCP SDK | rmcp 0.3.2 | Official Rust MCP SDK |
| Debug Interface | OpenOCD TCL socket | Persistent connection, no per-command process overhead |
| Serial | tokio-serial 5.4 | Async UART capture for console monitoring |
| Async | tokio 1 | Multi-session support |

## Tools (10)

| Tool | Args | Returns |
|------|------|---------|
| `connect` | cfg_file, extra_args? | session_id, ports (TCL/GDB/telnet) |
| `disconnect` | session_id | Confirmation |
| `get_status` | session_id | Target state, PC value |
| `halt` | session_id | Confirmation |
| `run` | session_id | Confirmation |
| `reset` | session_id, halt_after_reset? | Confirmation |
| `load_firmware` | session_id, file_path, address? | Load result (bytes written) |
| `read_memory` | session_id, address, count?, format? | Memory contents (hex or words32) |
| `write_memory` | session_id, address, value | Confirmation |
| `monitor` | session_id, port?, baud_rate?, duration_seconds? | Captured serial output |

### connect

Starts an OpenOCD daemon with the given `.cfg` file, allocates 3 ports (TCL, GDB, telnet), connects to the TCL socket, and returns a session_id. The OpenOCD process runs as a child and is killed on `disconnect`.

### load_firmware

Halts the target, then uses `load_image` to load firmware to RAM. Handles three formats:
- **ELF**: Auto-detected, addresses from ELF sections
- **HEX**: Intel HEX format, addresses embedded
- **BIN**: Requires explicit `address` parameter (e.g., `"0x10000000"` for MCUSRAM)

### monitor

Opens a UART serial port and captures output for `duration_seconds`. Returns the captured text. The serial port can be specified per-call or defaulted via `--serial-port` CLI flag.

## Architecture

```
┌──────────────────────────────┐
│  MCP Tool Layer (10 tools)   │
├──────────────────────────────┤
│  Session Manager             │
│  HashMap<UUID, OpenocdSession│
│  PortAllocator (6666+3N)     │
├──────────────────────────────┤
│  OpenocdClient               │
│  TCL socket (0x1a protocol)  │
│  Process lifecycle           │
├──────────────────────────────┤
│  OpenOCD daemon (subprocess) │
│  -f board.cfg               │
└──────────────────────────────┘
```

### TCL Protocol

OpenOCD exposes a TCL command interface over TCP. Commands and responses are UTF-8 text terminated by `0x1a` (ASCII SUB). This is more efficient than spawning `openocd` per command — one persistent TCP connection handles all interactions.

### Port Allocation

Each `connect()` call allocates 3 consecutive ports starting from 6666. The allocator increments by 3 per session and wraps at 60000. This prevents port conflicts when multiple sessions are active.

## Key Design Decisions

1. **TCL socket over CLI spawning**: A single TCP connection persists for the session lifetime. No process spawning overhead per command. Matches how OpenOCD is designed to be used programmatically.

2. **`load_image` not `flash write_image`**: The M4 core on STM32MP1 has no persistent flash. RETRAM (64KB) and MCUSRAM (384KB) are volatile — firmware must be reloaded on every power cycle. `load_image` writes to memory without flash erase cycles.

3. **Board-agnostic design**: No STM32MP1-specific code. All board behavior comes from the OpenOCD `.cfg` file. Reusable for any OpenOCD-supported target (STM32, ESP32 JTAG, RISC-V, etc.).

4. **Session model matches embedded-probe**: Same pattern as the probe-rs MCP — `connect()` returns session_id, all tools take session_id. Familiar API for users of both servers.

5. **UART monitor via tokio-serial**: Separate from OpenOCD's debug channel. Captures real UART console output (e.g., Zephyr shell on usart3) rather than semihosting or RTT.

## Testing

**16 tests** — all pass without OpenOCD or hardware:

| Category | Count | Description |
|----------|-------|-------------|
| Config | 3 | PATH search, bad path, which() helper |
| TCL Client | 9 | Address/memory parsing, protocol constants |
| Main | 4 | Arg parsing, config creation |

```bash
cd claude-mcps/openocd-debug && cargo test
```
