# Plan: Extend Embedded-Debugger MCP Server

## Overview

Extend the existing embedded-debugger MCP at `/Users/danahern/code/claude/work/claude-mcps/embedded-probe` to add boot validation, custom target support, and vendor tool integration.

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                    MCP Tools Layer (36 tools)                    │
├───────────────┬───────────────┬────────────────┬────────────────┤
│  probe-rs     │ Vendor Tools  │  Validation &  │  Analysis &    │
│  (primary)    │ (subprocess)  │  Workflows     │  Diagnostics   │
├───────────────┼───────────────┼────────────────┼────────────────┤
│ • connect     │ • esptool_    │ • validate_    │ • stack_trace  │
│ • flash_*     │   flash       │   boot         │ • core_dump    │
│ • reset/halt  │ • esptool_    │ • run_firmware │ • analyze_     │
│ • rtt_*       │   monitor     │                │   coredump     │
│ • read/write  │ • nrfjprog_   │                │ • resolve_     │
│   _memory     │   flash       │                │   symbol       │
│ • set/clear_  │               │                │ • gdb_server   │
│   breakpoint  │               │                │                │
│ • watchpoints │               │                │                │
└───────────────┴───────────────┴────────────────┴────────────────┘
```

**Strategy:**
- probe-rs handles most chips (STM32, nRF52/53/54, ESP32-C series)
- Vendor tools as fallback for specific needs (esptool for ESP32 Xtensa, nrfjprog for Nordic-specific features)
- Boot validation orchestrates flash + reset + RTT monitoring

---

## Phase 1 Tools (5 total)

### 1. `validate_boot` - Boot Validation
Flash, reset, and verify device boots successfully via RTT pattern matching.

```rust
ValidateBootArgs {
    session_id: String,
    file_path: String,
    success_pattern: String,      // e.g., "Boot complete" or regex
    timeout_ms: u32,              // default: 10000
    rtt_channel: u32,             // default: 0
    capture_output: bool,         // default: true
}

BootValidationResult {
    success: bool,
    boot_time_ms: u64,
    matched_pattern: Option<String>,
    rtt_output: Option<String>,
    error_messages: Vec<String>,
}
```

### 2. `load_custom_target` - Custom Target Support
Load CMSIS-pack generated target files for chips not yet in probe-rs builtin list.

```rust
LoadCustomTargetArgs {
    target_file_path: String,     // Path to YAML target file
    family_name: String,          // e.g., "Custom_MCU"
}
```

*Note: Most common chips (STM32, nRF, ESP32, Alif) are already supported. This is for edge cases.*

### 3. `esptool_flash` - ESP32 via esptool
Flash ESP32 Xtensa devices via serial using esptool.

```rust
EsptoolFlashArgs {
    port: String,                 // e.g., "/dev/ttyUSB0"
    file_path: String,
    chip: String,                 // esp32, esp32s2, esp32s3
    baud_rate: u32,               // default: 921600
    verify: bool,                 // default: true
    reset_after: bool,            // default: true
}
```

### 4. `nrfjprog_flash` - Nordic via nrfjprog
Flash Nordic devices using nrfjprog (J-Link specific features).

```rust
NrfjprogFlashArgs {
    file_path: String,
    family: String,               // NRF52, NRF53, NRF54
    snr: Option<String>,          // Serial number (multi-device)
    verify: bool,
    reset_after: bool,
    sectorerase: bool,            // default: true (vs chiperase)
}
```

### 5. `esptool_monitor` - ESP32 Serial Monitor
Monitor ESP32 serial output (replaces RTT for ESP32 Xtensa).

```rust
EsptoolMonitorArgs {
    port: String,
    baud_rate: u32,               // default: 115200
    timeout_ms: u64,              // 0 = no timeout
    max_bytes: usize,             // default: 4096
}
```

---

## File Changes

### Simplified Architecture (No New Modules)
All tools implemented inline in existing files to minimize maintenance surface area.

### Modified Files
| File | Changes |
|------|---------|
| `src/tools/types.rs` | Add 6 new Args/Result structs |
| `src/tools/debugger_tools.rs` | Add 5 new tool implementations (~300 lines) |
| `Cargo.toml` | Add `regex = "1"` dependency |
| `README.md` | Update tool count, add chip matrix, vendor install docs |

---

## Implementation Details

### Boot Validation Flow
```
1. Get session (existing)
2. Flash firmware (existing flash_program)
3. Reset without halt
4. Attach RTT (existing rtt_attach)
5. Poll RTT channel in loop:
   - Check for success_pattern match
   - Accumulate output
   - Check timeout
6. Return BootValidationResult
```

### Vendor Tool Integration
Vendor tools called as subprocesses:
```rust
// esptool example
Command::new("esptool.py")
    .args(["--chip", &args.chip, "--port", &args.port])
    .args(["write_flash", "0x0", &args.file_path])
    .output()
```

Benefits:
- No Rust library dependencies for vendor tools
- Uses user's installed toolchain versions
- Easy to add new vendor tools

### Custom Target Loading
Uses probe-rs target file format:
```rust
use probe_rs::config::add_target_from_yaml;

pub fn load_target(path: &Path) -> Result<()> {
    let yaml = std::fs::read_to_string(path)?;
    add_target_from_yaml(&yaml)?;
    Ok(())
}
```

---

## Supported Chips Summary

| Chip Family | probe-rs | Vendor Tool | Notes |
|-------------|----------|-------------|-------|
| STM32 | ✅ Primary | - | Full support |
| nRF52/53/54 | ✅ Primary | nrfjprog | nrfjprog for J-Link features |
| ESP32-C3/C6 | ✅ Primary | esptool | RISC-V works well in probe-rs |
| ESP32/S2/S3 | ⚠️ Limited | esptool | Use esptool for reliable flash |
| Alif | ✅ Primary | - | CMSIS-DAP + J-Link flash configs |

---

## Verification Plan

### 1. validate_boot
```bash
# Flash and verify boot message appears
validate_boot(session_id, file_path="firmware.elf",
              success_pattern="Booting Zephyr", timeout_ms=5000)
```

### 2. esptool_flash
```bash
# Flash ESP32 via serial
esptool_flash(port="/dev/cu.usbserial-1430",
              file_path="zephyr.bin", chip="esp32")
```

### 3. nrfjprog_flash
```bash
# Flash nRF52840 via nrfjprog
nrfjprog_flash(file_path="zephyr.hex", family="NRF52")
```

### 4. Alif via probe-rs
```bash
# Alif works directly with probe-rs (CMSIS-DAP / J-Link)
connect(probe_selector="auto", target_chip="AE722F80F55D5XX")
flash_program(session_id, file_path="firmware.elf")
```

---

## Implementation Order

1. **validate_boot** - Uses existing RTT, adds pattern matching (most value)
2. **esptool_flash/monitor** - Subprocess wrapper for ESP32 Xtensa
3. **nrfjprog_flash** - Subprocess wrapper for Nordic
4. **load_custom_target** - Simple probe-rs API call (lowest priority)

---

## Dependencies

```toml
# Add to Cargo.toml
regex = "1"  # For pattern matching in validate_boot
```

No other new dependencies - vendor tools are subprocess calls.

---

## Documentation

### README Updates Required

Update `/Users/danahern/code/claude/work/claude-mcps/embedded-probe/README.md` with:

1. **Quick Start** - Simple examples for common workflows
2. **Tool Reference** - All 27 tools with plain-English descriptions
3. **Chip Support Matrix** - What works with what
4. **Vendor Tool Requirements** - How to install esptool, nrfjprog
5. **Troubleshooting** - Common issues and solutions

### Living Documentation

This plan will be updated as implementation progresses:
- [x] Add learnings from validate_boot implementation
- [x] Document any probe-rs quirks discovered
- [ ] Note esptool/nrfjprog version compatibility (needs testing)
- [ ] Track chip-specific gotchas (needs testing)

---

## Learnings Log

*(Updated during implementation)*

| Date | Learning |
|------|----------|
| 2026-02-01 | `add_target_from_yaml` takes a `Read` impl, not a string. Use `std::fs::File::open()` directly |
| 2026-02-01 | Simplified architecture: no separate modules for vendor tools. All tools inline in `debugger_tools.rs` reduces maintenance |
| 2026-02-01 | `esptool_monitor` uses Python script with pyserial instead of esptool.py monitor (simpler, more reliable) |
| 2026-02-01 | `validate_boot` reuses existing RTT infrastructure - attach, read loop with pattern matching, clean separation |
| 2026-02-01 | Regex crate handles both literal strings and regex patterns via `escape()` fallback |
| 2026-02-14 | Zephyr coredump register order is NOT R0-R12. It's R0,R1,R2,R3,R12,LR,PC,xPSR,SP,[R4-R11]. First 8 regs match the ARM exception frame layout. |
| 2026-02-14 | `ptr_size_bits` in the coredump header is log2 (5=32-bit, 6=64-bit), not the byte count. Memory block address sizes depend on this. |
| 2026-02-14 | `#CD:` lines may be split across multiple log lines at 64 hex chars (32 bytes) each. Parser must concatenate all hex data between BEGIN/END. |
| 2026-02-14 | Stack walking from coredump memory is heuristic — scan for odd addresses in flash range. Symbol resolution filters noise. |
| 2026-02-14 | Shared `debug_coredump.conf` overlay merges cleanly via `OVERLAY_CONFIG` — confirmed in build output. |
| 2026-02-14 | Zephyr's `LOG_PANIC()` call before coredump switches to synchronous logging, preventing RTT buffer overflow. |
| 2026-02-14 | nRF54L15 (Cortex-M33) fully supports `CONFIG_DEBUG_COREDUMP` — build includes all coredump sources. |

---

## Testing

### Running Tests

```bash
cd claude-mcps/embedded-probe && cargo test
```

All tests run without hardware connected.

### Test Coverage (48 library tests + integration tests)

**Coredump tests** (`src/coredump.rs` — 13 tests):
- ELF core dump generation: magic, headers, notes, load segments, register roundtrip, empty regions
- Zephyr coredump parser: register parsing, memory blocks, reason codes, error cases, log prefixes, crash report formatting

**Symbol resolver tests** (`src/symbols/resolver.rs` — 12 tests):
- Exact address, offset, out-of-range, between symbols, zero-size, Thumb bit, empty table, adjacent, display

**Debugger tool tests** (`src/tools/debugger_tools.rs` — 18 tests):
- DWT addresses, function encoding, mask encoding, stack trace address validation

**Other library tests** (5 tests):
- Probe discovery, RTT ELF parsing, probe type support

**Integration tests** (`tests/integration_tests.rs` — 9 tests):
- Config, probe discovery, error types, tool handler creation

---

## Implementation Status

### Phase 1: Vendor Tools & Boot Validation (5 tools) ✅ COMPLETE

1. ✅ `validate_boot` - Flash + reset + RTT pattern matching
2. ✅ `esptool_flash` - ESP32 Xtensa flashing via subprocess
3. ✅ `esptool_monitor` - Serial monitor via Python/pyserial
4. ✅ `nrfjprog_flash` - Nordic flashing via subprocess
5. ✅ `load_custom_target` - Custom target YAML loading

**Files Modified:**
- `src/tools/types.rs` - Added 6 new type definitions
- `src/tools/debugger_tools.rs` - Added 5 new tool implementations (~300 lines)
- `Cargo.toml` - Added `regex = "1"` dependency
- `README.md` - Updated to 27 tools, added chip matrix, vendor install instructions

### Phase 2: Advanced Debugging (8 tools) ✅ COMPLETE

1. ✅ `read_registers` - Read all ARM Cortex-M registers (R0-R12, SP, LR, PC, xPSR)
2. ✅ `write_register` - Write a single register by name
3. ✅ `resolve_symbol` - Resolve address to function name via ELF symbol table
4. ✅ `stack_trace` - Walk stack and resolve return addresses to symbols
5. ✅ `set_watchpoint` - Configure DWT data watchpoints (read/write/readwrite)
6. ✅ `clear_watchpoint` - Remove DWT watchpoints by index
7. ✅ `core_dump` - Capture registers + RAM to GDB-compatible ELF core file
8. ✅ `gdb_server` - Spawn probe-rs GDB server on configurable port

### Phase 3: Crash Debug & Coredump Analysis (1 tool) ✅ COMPLETE

1. ✅ `analyze_coredump` - Parse Zephyr `#CD:` coredump from RTT, resolve symbols, return crash report

**Total: 36 tools**

---

## Phase 3: Crash Debug & Coredump Analysis

### Goal

One-shot crash diagnosis: capture RTT output containing Zephyr's coredump data, parse it, resolve symbols, and return "what crashed and why" — no GDB, no manual hex parsing.

### Key Insight

Zephyr has a built-in coredump subsystem (`CONFIG_DEBUG_COREDUMP`). When a fault occurs, it captures the **exception frame registers** (the actual crash PC/LR/SP, not the fault handler's context) and memory regions, then outputs them as `#CD:` prefixed hex lines through the logging system. With RTT logging enabled, this data flows through RTT and can be captured by `rtt_read`.

This is better than halting after a fault and reading registers via probe-rs, because the exception frame contains the pre-fault register state.

### `analyze_coredump` Tool

**Input:** Raw RTT/log text containing `#CD:` lines + ELF path for symbols
**Output:** Structured crash report with function names, call chain, register dump

```
=== Crash Analysis Report ===

Fault reason: K_ERR_CPU_EXCEPTION
Crash PC:     0x0002A3F4 → sensor_read_register+0x8
Caller (LR):  0x0002A3E1 → sensor_process_data+0x1d
Stack (SP):   0x2000FE80

Register Dump:
  R0:  0x00000000   R1:  0x0002B100   R2:  0x0000BEEF   R3:  0x00000003
  ...

Call chain (from exception frame):
  #0 0x0002A3F4 sensor_read_register+0x8                  ← CRASH HERE
  #1 0x0002A3E1 sensor_process_data+0x1d
  #2 ...
```

### Zephyr Coredump Binary Format

RTT output contains `#CD:BEGIN#` ... `#CD:<hex>` ... `#CD:END#` lines. Each `#CD:` line (after prefix) is hex-encoded binary. The binary format:

#### File Header (12 bytes)
```
Offset  Size  Field
0       2     id[2]           = 'Z','E' (magic)
2       2     hdr_version     = 2 (uint16 LE)
4       2     tgt_code        = 3 for ARM Cortex-M (uint16 LE)
6       1     ptr_size_bits   = 5 for 32-bit (log2)
7       1     flag
8       4     reason          = 0=CPU_EXCEPTION, 1=SPURIOUS_IRQ, 2=STACK_CHK_FAIL, 3=KERNEL_OOPS, 4=KERNEL_PANIC
```

#### Architecture Block ('A')
```
Offset  Size  Field
0       1     id              = 'A' (0x41)
1       2     hdr_version     (uint16 LE)
3       2     num_bytes       (uint16 LE) — 36 for v1 (9 regs), 68 for v2 (17 regs)
5       N     register data   (uint32 LE each)
```

**Register order (v2, 17 regs):** R0, R1, R2, R3, R12, LR, PC, xPSR, SP, R4, R5, R6, R7, R8, R9, R10, R11

Note: R0-R3, R12, LR, PC, xPSR come from the hardware exception frame (the actual crash site). SP is the pre-exception stack pointer. R4-R11 are callee-saved registers captured by the fault handler.

#### Memory Block ('M')
```
Offset  Size  Field
0       1     id              = 'M' (0x4D)
1       2     hdr_version     (uint16 LE)
3       4     start           (uint32 LE for 32-bit targets)
7       4     end             (uint32 LE)
11      N     data            (end - start bytes)
```

#### Threads Metadata Block ('T') — optional, skipped by parser
```
Offset  Size  Field
0       1     id              = 'T'
1       2     hdr_version     (uint16 LE)
3       2     num_bytes       (uint16 LE)
5       N     data            (skipped)
```

### Implementation Details

**Parser** (`src/coredump.rs`):
- `extract_coredump_bytes()` — finds `#CD:BEGIN#`...`#CD:END#`, concatenates hex data, decodes to bytes
- `parse_zephyr_coredump()` — parses file header, architecture block (registers), memory blocks
- `format_crash_report()` — generates human-readable report with symbol resolution
- Handles Zephyr log prefixes before `#CD:` (e.g., `[00:00:05.123,456] <inf> coredump: #CD:...`)
- Stack walking: scans memory region covering SP for Thumb return addresses, resolves via symbol table

**Tool** (`src/tools/debugger_tools.rs`):
- No session required — works purely on text input + ELF file
- Calls `parse_zephyr_coredump()` then `format_crash_report()` with `SymbolTable::from_elf()`

**Types** (`src/tools/types.rs`):
```rust
pub struct AnalyzeCoredumpArgs {
    pub log_text: String,    // Raw RTT/log text with #CD: lines
    pub elf_path: String,    // Path to ELF for symbol resolution
}
```

### Shared Debug Config Library

**File:** `zephyr-apps/lib/debug_config/debug_coredump.conf`

Kconfig overlay that any Zephyr app can include to get crash-ready coredump support:

```ini
# Coredump subsystem
CONFIG_DEBUG_COREDUMP=y
CONFIG_DEBUG_COREDUMP_BACKEND_LOGGING=y
CONFIG_DEBUG_COREDUMP_MEMORY_DUMP_LINKER_RAM=y

# RTT logging (required for coredump output via debug probe)
CONFIG_LOG=y
CONFIG_USE_SEGGER_RTT=y
CONFIG_LOG_BACKEND_RTT=y
CONFIG_LOG_BACKEND_UART=n
CONFIG_RTT_CONSOLE=y
CONFIG_UART_CONSOLE=n

# Debug-friendly build settings
CONFIG_DEBUG_OPTIMIZATIONS=y
CONFIG_DEBUG_THREAD_INFO=y
CONFIG_EXTRA_EXCEPTION_INFO=y
```

**Usage:** One line in any app's `CMakeLists.txt`:
```cmake
list(APPEND OVERLAY_CONFIG "${CMAKE_CURRENT_LIST_DIR}/../../lib/debug_config/debug_coredump.conf")
```

### Demo App: `crash_debug`

**Location:** `zephyr-apps/apps/crash_debug/`

Minimal app that boots, waits 5 seconds, then crashes through a 4-function call chain (`main` → `sensor_init_sequence` → `sensor_process_data` → `sensor_read_register` → NULL pointer write → HardFault).

Demonstrates the full workflow: build → flash → crash → capture RTT → `analyze_coredump` → get crash report.

**Build:** 45 KB flash, 9 KB RAM on nRF54L15DK.

### End-to-End Workflow

```
1. Build:    zephyr-build.build(app="crash_debug", board="nrf54l15dk/nrf54l15/cpuapp", pristine=true)
2. Connect:  embedded-probe.connect(probe_selector="auto", target_chip="nRF54L15_xxAA")
3. Flash:    embedded-probe.validate_boot(session_id, file_path="...zephyr.elf", success_pattern="Crash debug app booted")
4. Wait:     ~5 seconds for crash + coredump output via RTT
5. Capture:  embedded-probe.rtt_read(session_id, timeout_ms=8000)
6. Analyze:  embedded-probe.analyze_coredump(log_text=<rtt_output>, elf_path="...zephyr.elf")
   → Returns: crash function, call chain, registers, fault reason — no GDB needed
```

### Test Coverage (7 new tests, 48 total)

**Zephyr coredump parser tests** (`src/coredump.rs`):
- `test_parse_zephyr_coredump_registers` — Full v2 register parsing (17 regs)
- `test_parse_zephyr_coredump_with_memory` — Memory block parsing (base address + data)
- `test_parse_zephyr_coredump_reason_codes` — All 5 reason codes + unknown
- `test_parse_zephyr_coredump_no_data` — Error on missing `#CD:` data
- `test_parse_zephyr_coredump_bad_magic` — Error on invalid magic bytes
- `test_parse_zephyr_coredump_with_log_prefixes` — Handles Zephyr log timestamp prefixes
- `test_format_crash_report_with_symbols` — Report with symbol resolution
- `test_format_crash_report_no_symbols` — Report without symbols (shows `???`)

Tests use synthetic `#CD:` data (helper builds binary coredump and encodes as hex log lines). No hardware needed.

### Files Changed

| File | Change |
|------|--------|
| `src/coredump.rs` | Added ~280 lines: parser, formatter, structs, 7 tests |
| `src/tools/types.rs` | Added `AnalyzeCoredumpArgs` struct |
| `src/tools/debugger_tools.rs` | Added `analyze_coredump` tool, updated count to 36 |

### Learnings

| Date | Learning |
|------|----------|
| 2026-02-14 | Zephyr coredump register order is NOT R0-R12. It's R0,R1,R2,R3,R12,LR,PC,xPSR,SP,[R4-R11]. First 8 regs match the ARM exception frame layout. |
| 2026-02-14 | `ptr_size_bits` in the header is log2 (5=32-bit, 6=64-bit), not the byte count. Memory block address sizes depend on this. |
| 2026-02-14 | `#CD:` lines may be split across multiple log lines at 64 hex chars (32 bytes) each. Parser must concatenate all hex data between BEGIN/END. |
| 2026-02-14 | Stack walking from coredump memory is heuristic — scan for odd addresses in flash range (Thumb bit set). Works well in practice but may include false positives. Symbol resolution filters most noise. |
| 2026-02-14 | The shared `debug_coredump.conf` overlay approach works cleanly — Zephyr's `OVERLAY_CONFIG` merges it after `prj.conf`, confirmed in build output. |

### Risks & Mitigations

- **RTT buffer overflow on large coredumps:** Zephyr calls `LOG_PANIC()` before coredump which switches to synchronous logging. Default `CONFIG_DEBUG_COREDUMP_MEMORY_DUMP_LINKER_RAM` only captures linker-defined RAM, not the full memory map.
- **nRF54L15 coredump support:** Confirmed working — ARM Cortex-M33 is supported. Build includes `coredump.c`, `coredump_core.c`, `coredump_backend_logging.c`.
- **Coredump format versioning:** Parser handles both v1 (9 regs) and v2 (17 regs) architecture blocks. File header version is checked but flexible.
