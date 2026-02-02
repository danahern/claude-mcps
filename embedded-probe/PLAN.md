# Plan: Extend Embedded-Debugger MCP Server

## Overview

Extend the existing embedded-debugger MCP at `/Users/danahern/code/claude/work/claude-mcps/embedded-probe` to add boot validation, custom target support, and vendor tool integration.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│              MCP Tools Layer (27 tools)             │
├──────────────────┬──────────────────┬───────────────┤
│   probe-rs       │  Vendor Tools    │  Validation   │
│   (primary)      │  (escape hatch)  │  (workflows)  │
├──────────────────┼──────────────────┼───────────────┤
│ • connect        │ • esptool_flash  │ • validate_   │
│ • flash_program  │ • nrfjprog_flash │   boot        │
│ • reset          │ • alif_flash     │               │
│ • rtt_*          │                  │               │
└──────────────────┴──────────────────┴───────────────┘
```

**Strategy:**
- probe-rs handles most chips (STM32, nRF52/53/54, ESP32-C series)
- Vendor tools as fallback for specific needs (esptool for ESP32 Xtensa, nrfjprog for Nordic-specific features)
- Boot validation orchestrates flash + reset + RTT monitoring

---

## New Tools (5 total)

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

---

## Implementation Status

**✅ COMPLETE** - All 5 new tools implemented and building:

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
