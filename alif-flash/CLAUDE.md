# alif-flash

Alif E7 MRAM flash MCP server — SE-UART ISP protocol at 57600 baud. Replaces the ad-hoc Python scripts and provides Claude direct flash programming without shelling out.

## Setup

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -e ".[dev]"
```

Requires PRG_USB cable connected to Alif E7 AppKit (JLink VCOM provides SE-UART).

## Tools

- `list_ports()` — List available `/dev/cu.usbmodem*` serial ports
- `probe(port?)` — Check if SE-UART is responsive, report ISP/maintenance mode status
- `maintenance(port?)` — Enter maintenance mode: START_ISP → SET_MAINTENANCE → RESET → verify
- `gen_toc(config)` — Run `app-gen-toc` to generate ATOC package from JSON config
- `flash(config, port?, maintenance?)` — Write all images to MRAM from ATOC JSON config

## Typical Workflow

```
1. alif-flash.list_ports()                    # Verify cable connected
2. alif-flash.probe()                         # Check SE status
3. alif-flash.maintenance()                   # Enter maintenance mode (power cycle first)
4. alif-flash.gen_toc(config="build/config/linux-boot-e7.json")  # Generate ATOC
5. alif-flash.flash(config="/path/to/linux-boot-e7.json")        # Flash all images
```

Or combined: `alif-flash.flash(config="...", maintenance=true)`

## Key Details

- ISP protocol: `[length, cmd, data..., checksum]` — all bytes sum to 0 mod 256
- Data transfer: 240-byte chunks with 2-byte LE sequence numbers
- MRAM addresses: TF-A@0x80002000, DTB@0x80010000, kernel@0x80020000, rootfs@0x80300000
- After flash: power cycle (unplug/replug PRG_USB) required for A32 boot — JLink reset doesn't trigger SE boot sequence
- SE-UART baud rate: 57600 (set in `isp_config_data.cfg`)
- Port auto-detection: uses first `/dev/cu.usbmodem*` if not specified

## J-Link Alternative (jlink/)

Direct MRAM programming via JLinkExe `loadbin` — ~44 KB/s, 9x faster than SE-UART.

**One-time setup:** `jlink/setup.sh` installs `Devices.xml` + `AlifE7.JLinkScript` to `~/Library/Application Support/SEGGER/JLinkDevices/AlifSemi/`. The JLinkScript overrides `ResetTarget()` to prevent `loadbin`'s implicit reset from killing the SE boot sequence.

**Usage:** `jlink/flash-mram.sh [-v] [-c component] [image_dir]`

Requires power cycle (unplug/replug PRG_USB) before first connection. Pending integration as MCP tool (Task #9).

## Testing

```bash
.venv/bin/python -m pytest tests/ -v
```

Tests cover checksum calculation, packet framing, response parsing, and protocol constants.
