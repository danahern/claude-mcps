# embedded-probe

Embedded debugging and flash programming MCP server. Primary backend: probe-rs (ARM Cortex-M, RISC-V, Xtensa via J-Link, ST-Link, CMSIS-DAP, ESP-USB-JTAG). Secondary backend: JLinkExe subprocess — auto-activates when probe-rs rejects a chip, covering any target in Segger's device DB including Cortex-A32.

## JLink Auto-Fallback Backend

When `connect()` is called with a J-Link probe and probe-rs fails to attach (e.g., for unsupported chips like Cortex-A32), the MCP **automatically retries using JLinkExe as a subprocess backend**. The `target_chip` argument is passed directly to JLinkExe as the device name. No custom YAML, no workaround — just call `connect()`.

**Operations available via JLink backend:**
- `halt`, `run`, `reset`, `step`, `get_status`
- `read_memory`, `write_memory`
- `read_registers`, `write_register`
- `flash_erase`, `flash_program` (uses JLink `loadbin`/`loadfile`)

**Not available via JLink backend:** RTT, breakpoints, watchpoints, `core_dump`, `stack_trace`, `analyze_coredump`

**Alif E7/E8 Cortex-A32 chip names (use with `connect()`):**
| Chip name | Core | Use |
|-----------|------|-----|
| `"Cortex-A32"` | Generic | Recommended for memory reads (JTAG, read-only after SE boot) |
| `"AE722F80F55D5_A32_0"` | E7 core 0 | More specific, same read-only constraint |
| `"AE722F80F55D5_A32_1"` | E7 core 1 | |
| `"AE822FA0E5597_A32_0"` | E8 core 0 | |
| `"AE822FA0E5597_A32_1"` | E8 core 1 | |

**CRITICAL**: Cortex-A32 MRAM is write-protected after SE boot. Use `read_memory`/`read_registers` only — do not attempt flash operations via the A32 core.

**CRITICAL**: Always call `list_targets` before `connect` — it lists known chip names including Alif A32 targets with their probe type and notes.

## Build

```bash
cargo build --release
```

Binary: `target/release/embedded-probe`

## Architecture

```
src/
├── main.rs              # MCP server entry point
├── tools/
│   ├── types.rs         # All tool argument/response structs
│   ├── debugger_tools.rs # Tool implementations (ServerHandler)
│   └── mod.rs
├── coredump.rs          # ELF core dump generation + Zephyr coredump parser
├── symbols.rs           # ELF symbol resolution (addr2line equivalent)
└── config.rs            # Server configuration
```

## Tools by Category

### Probe & Connection
- `list_probes` — Find connected debug probes
- `list_targets` — **List known-good target configs. Call BEFORE connect to get the correct chip name — do not guess.**
- `connect` — Attach to probe + target chip (returns session_id)
- `disconnect` — Release a debug session
- `probe_info` — Get session info
- `load_custom_target` — Load target definition from YAML

### Execution Control
- `halt` — Stop CPU
- `run` — Resume CPU
- `reset` — Reset target (with optional halt)
- `step` — Single instruction step
- `get_status` — CPU state (running/halted, PC)

### Memory & Registers
- `read_memory` — Read target memory (hex/binary/ascii/words)
- `write_memory` — Write target memory
- `read_registers` — Dump all CPU registers (R0-R12, SP, LR, PC, xPSR)
- `write_register` — Write a specific register

### Breakpoints & Watchpoints
- `set_breakpoint` — Hardware or software breakpoint
- `clear_breakpoint` — Remove breakpoint
- `set_watchpoint` — Data watchpoint (halt on memory read/write, DWT-based)
- `clear_watchpoint` — Remove watchpoint by comparator index

### Flash Programming
- `flash_erase` — Erase sectors or full chip
- `flash_program` — Program ELF/HEX/BIN to flash
- `flash_verify` — Verify flash contents
- `run_firmware` — Full deploy: erase + program + verify + run + RTT attach
- `validate_boot` — Flash + verify boot via RTT pattern matching

### RTT Communication
- `rtt_attach` — Connect to RTT control block
- `rtt_detach` — Disconnect RTT
- `rtt_read` — Read from target (up channel)
- `rtt_write` — Write to target (down channel)
- `rtt_channels` — List available RTT channels

### Crash Analysis & Debugging
- `analyze_coredump` — Parse Zephyr `#CD:` coredump from RTT, resolve symbols, return crash report
- `resolve_symbol` — Map address to function name + source line via ELF
- `stack_trace` — Walk stack frames with symbol resolution
- `core_dump` — Dump registers + RAM to file (GDB-compatible ELF with elf_path)

### Vendor Tools (require external CLIs)
- `esptool_flash` — Flash ESP32 via esptool
- `esptool_monitor` — Read ESP32 serial output
- `nrfjprog_flash` — Flash Nordic devices via nrfjprog
- `nrfutil_program` — Flash Nordic devices via nrfutil (nRF5340 dual-core support)
- `nrfutil_recover` — Clear APPROTECT via nrfutil
- `nrfutil_reset` — Reset device via nrfutil
- `gdb_server` — Start probe-rs GDB server

### Vendor Tool Dependencies

These tools shell out to external CLIs. They fail at call time with an install hint if the CLI is missing.

| Tool | Requires | Install |
|------|----------|---------|
| `esptool_flash` | `esptool` | `pip install esptool` |
| `esptool_monitor` | `pyserial` | `pip install pyserial` |
| `nrfjprog_flash` | `nrfjprog` | [nRF Command Line Tools](https://www.nordicsemi.com/Products/Development-tools/nRF-Command-Line-Tools) |
| `nrfutil_program/recover/reset` | `nrfutil` + `device` subcommand | [nRF Util](https://www.nordicsemi.com/Products/Development-tools/nRF-Util), then `nrfutil install device` |

## Key Implementation Details

- **Session management**: `connect` returns a `session_id` used by all subsequent tool calls. Multiple sessions can be active.
- **Coredump parser** (`coredump.rs`): Parses Zephyr's binary coredump format (version 2, ARM Cortex-M). Extracts exception frame registers (PC/LR/SP at crash site, not fault handler).
- **Symbol resolution** (`symbols.rs`): Reads ELF symbol tables and DWARF debug info. Equivalent to `addr2line` but integrated.
- **probe-rs**: All hardware interaction goes through the probe-rs library. Target chip names must match probe-rs's target database.
- **CRITICAL: Always call `list_targets` before `connect`**. Do NOT guess or fabricate chip names. Wrong names cause JLink GUI dialogs that block execution. The `list_targets` tool returns verified chip names from real hardware testing.
