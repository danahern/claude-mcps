# alif-flash

Alif E7 flash MCP server — three methods: SE-UART ISP, J-Link loadbin, and RTT OSPI programmer.

## CRITICAL: Use the fastest available method

| Method | Speed | Use case |
|--------|-------|----------|
| `ospi_program` (RTT) | ~500 KB/s | **OSPI images — preferred** |
| `jlink_flash` | ~44 KB/s (MRAM), ~7 KB/s (OSPI) | MRAM images, OSPI fallback |
| `flash` (SE-UART) | ~5 KB/s | Initial ATOC setup only |

For OSPI images: ALWAYS use `ospi_program()` (70x faster than JLink FLM).
For MRAM images: use `jlink_flash()`.
For initial ATOC setup: use SE-UART `flash()`.

## Setup

```bash
pip install -e ".[dev]"
```

Requires PRG_USB cable connected to Alif E7 AppKit.

## Tools

### RTT OSPI Programmer (~500 KB/s)
- `ospi_program(config?, image?, address?, verify?)` — Program OSPI flash via RTT

### SE-UART (ISP protocol)
- `list_ports()` — List available serial ports
- `probe(port?)` — Check if SE-UART is responsive
- `maintenance(port?)` — Enter maintenance mode
- `gen_toc(config)` — Generate ATOC package from JSON config
- `flash(config, port?, maintenance?)` — Write all images via SE-UART (~5 KB/s)
- `monitor(port?, baud?, duration?)` — Read serial console output

### J-Link (direct loadbin)
- `jlink_setup(install?)` — Check or install J-Link device definition
- `jlink_flash(image_dir?, config?, components?, verify?)` — Flash via J-Link (~44 KB/s)

## Typical Workflows

### OSPI flash via RTT (fastest — preferred for OSPI)

**Step 1 — One-time ATOC setup with RTT programmer firmware:**
```
1. alif-flash.gen_toc(config="build/config/linux-boot-e7-ospi-rtt.json")
2. alif-flash.maintenance()
3. alif-flash.flash(config="linux-boot-e7-ospi-rtt.json")
   # Power cycle board
```

**Step 2 — OSPI programming via RTT (repeatable, ~500 KB/s):**
```
1. alif-flash.ospi_program(config="/path/to/linux-boot-e7-ospi-jlink.json", verify=true)
   # Power cycle board after flash
```

### MRAM-only (J-Link)
```
1. alif-flash.jlink_setup(install=true)
2. alif-flash.jlink_flash(config="/path/to/linux-boot-e7.json", verify=true)
   # Power cycle board after flash
```

### OSPI flash via J-Link FLM (fallback — slower)

**Step 1 — One-time ATOC setup with debug stub:**
```
1. alif-flash.gen_toc(config="build/config/linux-boot-e7-ospi.json")
2. alif-flash.maintenance()
3. alif-flash.flash(config="linux-boot-e7-ospi.json")
   # Power cycle board
```

**Step 2 — OSPI programming via J-Link FLM (~7 KB/s):**
```
1. alif-flash.jlink_setup(install=true)
2. alif-flash.jlink_flash(config="/path/to/linux-boot-e7-ospi-jlink.json", verify=true)
   # Power cycle board after flash
```

### SE-UART (fallback — reliable)
```
1. alif-flash.probe()
2. alif-flash.maintenance()
3. alif-flash.gen_toc(config="build/config/linux-boot-e7.json")
4. alif-flash.flash(config="/path/to/linux-boot-e7.json")
```

## Key Details

- RTT: ~500 KB/s via OSPI programmer firmware on M55_HP. Requires `linux-boot-e7-ospi-rtt.json` ATOC.
- J-Link: ~44 KB/s (MRAM), ~7 KB/s (OSPI via FLM). Requires freshly power-cycled board.
- SE-UART: ~5 KB/s via ISP protocol. 240-byte chunks, 2-byte LE sequence numbers.
- MRAM addresses: TF-A@0x80002000, DTB@0x80010000, kernel@0x80020000, rootfs@0x80300000
- OSPI addresses: rootfs@0xC0000000, kernel@0xC0800000 (IS25WX256 NOR flash)
- After flash: power cycle (unplug/replug PRG_USB) required for A32 boot

## Testing

```bash
python3 -m pytest tests/ -v
```

93 tests: ISP protocol (18) + J-Link (38) + OSPI RTT (37).
