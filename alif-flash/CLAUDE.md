# alif-flash

Alif Ensemble flash MCP server — supports E7 and E8 boards via `device` parameter.

## CRITICAL: Flash methods are under systematic re-validation

**All previous claims about which method works, persistence, speed, etc. have been deprecated.** See `plans/alif-flash-reset.md` for the systematic test plan. Do not make assumptions about flash behavior until tests are complete.

## Setup

```bash
pip install -e ".[dev]"
```

Requires PRG_USB cable connected to Alif E7 or E8 board.

## Tools

### SE-UART (ISP protocol)
- `list_ports()` — List available serial ports
- `probe(port?)` — Check if SE-UART is responsive
- `maintenance(port?)` — Enter maintenance mode
- `gen_toc(config)` — Generate ATOC package from JSON config
- `flash(config, port?, maintenance?)` — Write all images via SE-UART
- `monitor(port?, baud?, duration?)` — Read serial console output

### J-Link (direct loadbin)
- `jlink_setup(install?)` — Check or install J-Link device definition
- `jlink_flash(image_dir?, config?, components?, verify?)` — Flash via J-Link

### USB OSPI Programmer (XMODEM)
- `ospi_program_usb(image, device?, timeout?)` — Program OSPI via USB CDC-ACM XMODEM transfer. Auto-detects Alif CDC-ACM (VID 0x0525). Four timeout layers: receiver ready (30s), per-block ACK (10s), post-EOT completion (30s), overall (auto from file size). ~47 KB/s.

### RTT OSPI Programmer
- `ospi_program(config?, image?, address?, verify?)` — Status unknown, needs re-testing

## Official Workflow (from AUGD0022)

The official Alif flash workflow is:
1. `app-gen-toc -f config.json` → generates AppTocPackage.bin
2. `sudo ./app-write-mram -p` → writes ATOC to MRAM via SE-UART ISP
3. Power cycle → SE processes ATOC and boots configured cores

Our MCP wraps steps 1-2 as `gen_toc()` + `flash()`.

**Key detail:** The official docs use `-p` flag (16-byte alignment padding) on `app-write-mram`. Verify our MCP passes this.

## Key Details

- All tools accept optional `device` parameter: `"alif-e7"` (default) or `"alif-e8"`
- MRAM addresses (E7): TF-A@0x80002000, DTB@0x80010000, kernel@0x80020000, rootfs@0x80380000
- MRAM addresses (E8): same except rootfs@0x80380000
- OSPI addresses: rootfs@0xC0000000, kernel@0xC0800000 (IS25WX256 NOR flash)
- ISP baud rate: 57600 (AUGD0005 p.19)
- Console baud rate: 115200

## Testing

```bash
python3 -m pytest tests/ -v
```

169 tests: ISP protocol (23) + J-Link (45) + OSPI RTT (37) + Device registry (17) + XMODEM (47).
