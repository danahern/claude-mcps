# alif-flash

Alif E7 MRAM flash MCP server — two methods: SE-UART ISP (57600 baud) and J-Link loadbin (~44 KB/s).

## CRITICAL: Always use J-Link for flashing images

**NEVER use `flash()` (SE-UART) for image updates. ALWAYS use `jlink_flash()` instead.**

- `jlink_flash`: ~44 KB/s, all 4 images in ~78 seconds
- `flash` (SE-UART): ~5 KB/s, all 4 images in ~19 minutes

The ONLY reason to use SE-UART `flash()` is for initial ATOC setup (first-ever flash) or if J-Link is physically unavailable. For all routine image flashing, use `jlink_flash`.

## Setup

```bash
pip install -e ".[dev]"
```

Requires PRG_USB cable connected to Alif E7 AppKit.

## Tools

### SE-UART (ISP protocol)
- `list_ports()` — List available serial ports
- `probe(port?)` — Check if SE-UART is responsive
- `maintenance(port?)` — Enter maintenance mode
- `gen_toc(config)` — Generate ATOC package from JSON config
- `flash(config, port?, maintenance?)` — Write all images via SE-UART (~5 KB/s)
- `monitor(port?, baud?, duration?)` — Read serial console output

### J-Link (direct loadbin)
- `jlink_setup(install?)` — Check or install J-Link device definition
- `jlink_flash(image_dir?, config?, components?, verify?)` — Flash via J-Link (~44 KB/s, 9x faster)

## Typical Workflows

### MRAM-only (J-Link — preferred)
```
1. alif-flash.jlink_setup(install=true)                     # One-time setup
2. alif-flash.jlink_flash(config="/path/to/linux-boot-e7.json", verify=true)
   # Power cycle board after flash
```

### OSPI flash (kernel + rootfs in OSPI NOR)

Two-step process: SE-UART programs the ATOC (one-time), J-Link programs OSPI (iterative).

**Step 1 — One-time ATOC setup (SE-UART):**
The OSPI ATOC config includes an M55_HP debug stub so JLink can halt M55_HP to execute the OSPI flash algorithm (FLM).
```
1. alif-flash.gen_toc(config="build/config/linux-boot-e7-ospi.json")
2. alif-flash.maintenance()
3. alif-flash.flash(config="linux-boot-e7-ospi.json")
   # Power cycle board
```

**Step 2 — OSPI programming (J-Link, repeatable):**
```
1. alif-flash.jlink_setup(install=true)                     # One-time setup
2. alif-flash.jlink_flash(config="/path/to/linux-boot-e7-ospi-jlink.json", verify=true)
   # Power cycle board after flash
```

The JLink config has TFA + DTB disabled (they're in MRAM via ATOC). Only KERNEL and ROOTFS are active, targeting OSPI addresses (rootfs@0xC0000000, kernel@0xC0800000). OSPI writes are slower than MRAM (~7 KB/s, ~600s timeout for erase/program cycles).

### SE-UART (fallback — reliable)
```
1. alif-flash.probe()
2. alif-flash.maintenance()
3. alif-flash.gen_toc(config="build/config/linux-boot-e7.json")
4. alif-flash.flash(config="/path/to/linux-boot-e7.json")
```

## Key Details

- J-Link: ~44 KB/s via loadbin. Requires freshly power-cycled board. Auto-installs device definition.
- SE-UART: ~5 KB/s via ISP protocol. 240-byte chunks, 2-byte LE sequence numbers.
- MRAM addresses: TF-A@0x80002000, DTB@0x80010000, kernel@0x80020000, rootfs@0x80300000
- OSPI addresses: rootfs@0xC0000000, kernel@0xC0800000 (IS25WX512M NOR flash)
- OSPI requires M55_HP debug stub in ATOC — JLink uses M55_HP to run the FLM flash algorithm
- After flash: power cycle (unplug/replug PRG_USB) required for A32 boot

## Testing

```bash
python3 -m pytest tests/ -v
```

44 tests: ISP protocol (checksum, framing, parsing) + J-Link (output parsing, layout, setup, config parsing).
