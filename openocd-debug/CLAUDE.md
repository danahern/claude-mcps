# openocd-debug

Generic OpenOCD MCP server using TCL socket protocol. Board-specific behavior comes from the `.cfg` file passed to `connect()`.

## Build

```bash
cargo build --release
```

## Architecture

```
src/
├── main.rs              # MCP server entry point, logging setup
├── lib.rs               # Module declarations
├── config.rs            # CLI args, Config, which() helper
├── openocd_client.rs    # TCL socket client (0x1a terminator protocol)
└── tools/
    ├── mod.rs
    ├── types.rs          # 10 arg structs (ConnectArgs, ReadMemoryArgs, etc.)
    └── openocd_tools.rs  # #[tool_router] impl, session management, 10 tools
```

## Tools

- `connect(cfg_file, extra_args?)` — Start OpenOCD, connect TCL, return session_id
- `disconnect(session_id)` — Graceful shutdown + kill
- `get_status(session_id)` — `targets` + `reg pc`
- `halt(session_id)` — `halt`
- `run(session_id)` — `resume`
- `reset(session_id, halt_after_reset?)` — `reset halt` or `reset run`
- `load_firmware(session_id, file_path, address?)` — `load_image` (halts first, handles ELF/HEX/BIN)
- `read_memory(session_id, address, count?, format?)` — `mdw`, output as hex or words32
- `write_memory(session_id, address, value)` — `mww`
- `monitor(session_id, port?, baud_rate?, duration_seconds?)` — UART capture via tokio-serial

## Key Implementation Details

- **TCL protocol**: Commands sent as UTF-8 + 0x1a terminator byte. Responses read until 0x1a. Port 6666 default.
- **Port allocation**: Each session gets 3 consecutive ports (TCL, GDB, telnet). PortAllocator increments by 3, wraps at 60000.
- **Session management**: `connect()` spawns OpenOCD child process, waits for TCL port (exponential backoff, 5s timeout), returns UUID session_id. All tools take session_id. Sessions stored in `Arc<RwLock<HashMap>>`.
- **Process lifecycle**: OpenOCD child process is owned by `OpenocdClient`. `disconnect()` sends TCL `shutdown`, waits 200ms, then `kill()`.
- **load_firmware**: Halts target first. ELF auto-detected, HEX uses `ihex` format flag, BIN requires explicit address.
- **monitor**: Opens serial port via tokio-serial, reads until duration expires. Returns captured output as text.

## Testing

16 unit tests, no hardware needed:

```bash
cargo test
```

| Module | Tests | What |
|--------|-------|------|
| `config` | 3 | `which()` finds `ls`, rejects nonexistent, bad path error |
| `openocd_client` | 9 | Address parsing (hex, decimal, whitespace, invalid), memory dump parsing (single/multi-line, noise, empty), terminator value |
| `main` | 4 | Arg parsing defaults, with options, config creation, default config |
