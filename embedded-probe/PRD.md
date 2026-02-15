# PRD: embedded-probe MCP Server

## Purpose

Debug probe interface for ARM Cortex-M, RISC-V, and vendor-specific embedded targets. Provides Claude Code with 27 tools for connecting to debug probes, flashing firmware, setting breakpoints, reading/writing memory, and communicating via RTT — without the developer touching GDB, OpenOCD, or vendor CLIs.

## Technology Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Language | Rust | Performance, probe-rs is a Rust library |
| MCP SDK | rmcp 0.3.2 | Official Rust MCP SDK |
| Debug Engine | probe-rs 0.25 | Native Rust, multi-architecture, no subprocess overhead |
| Vendor: ESP32 | esptool (subprocess) | Xtensa not fully supported in probe-rs |
| Vendor: Nordic | nrfjprog (subprocess) | J-Link specific features |
| ELF Parsing | goblin 0.8 | RTT symbol lookup |
| Async | tokio 1.41 | Required by rmcp |

## Tools (27)

### Probe Management (3)

| Tool | Args | Returns |
|------|------|---------|
| `list_probes` | — | Array of {name, type, serial} for all connected probes |
| `connect` | probe_selector, target_chip, speed_khz, connect_under_reset, halt_after_connect | session_id |
| `probe_info` | session_id | Probe details, target info, connection state |

### Debug Control (4)

| Tool | Args | Returns |
|------|------|---------|
| `halt` | session_id | CPU halted confirmation |
| `run` | session_id | CPU running confirmation |
| `reset` | session_id, reset_type (hw/sw), halt_after_reset | Reset confirmation |
| `step` | session_id | Single step confirmation, new PC |

### Memory Operations (2)

| Tool | Args | Returns |
|------|------|---------|
| `read_memory` | session_id, address, size, format (hex/binary/ascii/words16/words32) | Memory contents |
| `write_memory` | session_id, address, data, format | Write confirmation |

### Breakpoints (2)

| Tool | Args | Returns |
|------|------|---------|
| `set_breakpoint` | session_id, address, breakpoint_type (hw/sw) | Breakpoint set confirmation |
| `clear_breakpoint` | session_id, address | Breakpoint cleared confirmation |

### Flash Programming (3)

| Tool | Args | Returns |
|------|------|---------|
| `flash_erase` | session_id, erase_type (all/sectors), address, size | Erase confirmation |
| `flash_program` | session_id, file_path, format (auto/elf/hex/bin), base_address, verify | Program confirmation |
| `flash_verify` | session_id, address, size, file_path or data | Verify result |

### RTT Communication (6)

| Tool | Args | Returns |
|------|------|---------|
| `rtt_attach` | session_id, control_block_address, memory_ranges | RTT attached |
| `rtt_detach` | session_id | RTT detached |
| `rtt_channels` | session_id | List of {index, name, direction, size} |
| `rtt_read` | session_id, channel, timeout_ms, max_bytes | Data from target |
| `rtt_write` | session_id, data, channel, encoding (utf8/hex/binary) | Write confirmation |
| `run_firmware` | session_id, file_path, format, reset_after_flash, attach_rtt, rtt_timeout_ms | Full deployment result |

### Workflow Tools (2)

| Tool | Args | Returns |
|------|------|---------|
| `get_status` | session_id | CPU state, halted/running, PC, session info |
| `disconnect` | session_id | Session closed |

### Vendor Tools (4)

| Tool | Args | Returns |
|------|------|---------|
| `esptool_flash` | port, file_path, chip, baud_rate, verify, reset_after | Flash result |
| `esptool_monitor` | port, baud_rate, timeout_ms, max_bytes | Serial output |
| `nrfjprog_flash` | file_path, family, snr, verify, sectorerase, reset_after | Flash result |
| `load_custom_target` | target_file_path | Target loaded confirmation |

## Hardware Support

### Debug Probes

| Probe | Protocol | Notes |
|-------|----------|-------|
| J-Link | SWD, JTAG | All Segger variants |
| ST-Link V2/V3 | SWD | STM32 development boards |
| DAPLink | CMSIS-DAP | ARM compatible |
| CMSIS-DAP | SWD, JTAG | Generic ARM debug |
| Black Magic Probe | GDB serial | Open-source probe |

### Target Chips

| Family | Architecture | probe-rs Support | Vendor Fallback |
|--------|-------------|------------------|-----------------|
| STM32 | ARM Cortex-M | Full | — |
| nRF52/53/54 | ARM Cortex-M | Full | nrfjprog |
| ESP32-C3/C6 | RISC-V | Full | esptool |
| ESP32/S2/S3 | Xtensa | Limited | esptool (recommended) |

## Architecture

```
┌──────────────────────────────────────┐
│  MCP Tool Layer (27 tools)           │
├────────────┬────────────┬────────────┤
│ probe-rs   │ Vendor     │ Validation │
│ (primary)  │ (fallback) │ (workflow) │
│            │            │            │
│ connect    │ esptool    │ validate_  │
│ flash_*    │ nrfjprog   │ boot      │
│ rtt_*      │            │ run_      │
│ halt/run   │            │ firmware  │
│ memory_*   │            │            │
│ breakpoint │            │            │
├────────────┴────────────┴────────────┤
│ Session Manager (HashMap<id, State>) │
└──────────────────────────────────────┘
```

### Session Model

`connect` creates a session with a UUID. All subsequent tools reference `session_id`. The session holds the probe-rs `Session` and `Core` objects, RTT state, and any attached breakpoints. `disconnect` releases everything cleanly.

## Key Design Decisions

1. **probe-rs as library, not subprocess**: Direct Rust API calls are faster and more reliable than shelling out to `probe-rs` CLI. The library provides session management, memory access, and RTT natively.

2. **Vendor tools as subprocess fallback**: `esptool.py` and `nrfjprog` are invoked via `Command::new()`. No Python or C library dependencies — just needs the binaries on PATH.

3. **validate_boot as highest-value tool**: A single MCP call does: flash ELF → reset → attach RTT → match pattern → return boot time. This replaces a 5-step manual workflow.

4. **Custom target YAML**: `load_custom_target` allows supporting chips not yet in the probe-rs registry by loading CMSIS-Pack style YAML definitions.

## Testing

**14 tests** — all pass without hardware:

| Category | Count | Description |
|----------|-------|-------------|
| Library | 3 | Probe type support, RTT address parsing, probe discovery |
| Unit | 2 | Default config, CLI args parsing |
| Integration | 9 | Config validation, probe discovery, error types, MCP handlers |

```bash
cd claude-mcps/embedded-probe && cargo test
```
