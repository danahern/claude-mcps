# alif-flash

Alif E7 flash MCP server — three methods: SE-UART ISP, J-Link loadbin, and RTT OSPI programmer.

## CRITICAL: Use the fastest available method

| Method | Speed | Use case |
|--------|-------|----------|
| `jlink_flash` | ~44 KB/s (MRAM), ~7 KB/s (OSPI) | **MRAM + OSPI images — preferred** |
| `flash` (SE-UART) | ~5 KB/s | Initial ATOC setup only |
| `ospi_program` (RTT) | **BROKEN** | Do not use — see below |

For OSPI images: use `jlink_flash()` (requires M55_HP debug stub in ATOC).
For MRAM images: use `jlink_flash()`.
For initial ATOC setup: use SE-UART `flash()`.

**`ospi_program` is broken:** M55_HP CPU cannot access the OSPI controller (BusFault due to EXPMST bridge not forwarding 0x8xxx_xxxx addresses). The firmware loads and RTT connects, but flash operations hang. See knowledge item k-c3cbe077.

## Setup

```bash
pip install -e ".[dev]"
```

Requires PRG_USB cable connected to Alif E7 AppKit.

## Tools

### RTT OSPI Programmer (BROKEN — do not use)
- `ospi_program(config?, image?, address?, verify?)` — **BROKEN:** M55_HP BusFault on OSPI access

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

### ~~OSPI flash via RTT~~ (BROKEN — M55_HP BusFault on OSPI controller access)

Do not use. M55_HP cannot access the OSPI controller due to EXPMST bridge configuration.
Use J-Link FLM workflow below instead.

### MRAM-only (J-Link)
```
1. alif-flash.jlink_setup(install=true)
2. alif-flash.jlink_flash(config="/path/to/linux-boot-e7.json", verify=true)
   # Power cycle board after flash
```

### OSPI flash via J-Link FLM (preferred for OSPI)

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

- RTT: **BROKEN** — M55_HP BusFault on OSPI controller access (EXPMST bridge issue). Do not use.
- J-Link: ~44 KB/s (MRAM), ~7 KB/s (OSPI via FLM). Requires freshly power-cycled board. **Preferred for OSPI.**
- SE-UART: ~5 KB/s via ISP protocol. 240-byte chunks, 2-byte LE sequence numbers.
- MRAM addresses: TF-A@0x80002000, DTB@0x80010000, kernel@0x80020000, rootfs@0x80300000
- OSPI addresses: rootfs@0xC0000000, kernel@0xC0800000 (IS25WX256 NOR flash)
- After flash: power cycle (unplug/replug PRG_USB) required for A32 boot

## Testing

```bash
python3 -m pytest tests/ -v
```

93 tests: ISP protocol (18) + J-Link (38) + OSPI RTT (37).
