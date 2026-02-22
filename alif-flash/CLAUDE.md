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

### J-Link (preferred — fast)
```
1. alif-flash.jlink_setup(install=true)                     # One-time setup
2. alif-flash.jlink_flash(config="/path/to/linux-boot-e7.json", verify=true)
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

- J-Link: ~44 KB/s via loadbin. Requires freshly power-cycled board. Auto-installs device definition.
- SE-UART: ~5 KB/s via ISP protocol. 240-byte chunks, 2-byte LE sequence numbers.
- MRAM addresses: TF-A@0x80002000, DTB@0x80010000, kernel@0x80020000, rootfs@0x80300000
- After flash: power cycle (unplug/replug PRG_USB) required for A32 boot

## Testing

```bash
python3 -m pytest tests/ -v
```

33 tests: ISP protocol (checksum, framing, parsing) + J-Link (output parsing, layout, setup).
