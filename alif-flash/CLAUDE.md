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

## USB OSPI Programming Workflow

Programs OSPI flash at ~47 KB/s via USB XMODEM (vs ~3 KB/s via JLink OSPI flash loader). Requires two firmware binaries that are NOT included in this MCP — they must be built from source.

### Prerequisites

**1. bl32-usbinit.bin** — TF-A variant that enables USB PHY then parks A32 in WFE.

```bash
# Build in tfa-build Docker container (source: alif_arm-tf repo)
cd firmware/linux/alif-e7
./build-tfa.sh --usb-init    # → tools/setools/build/images/bl32-usbinit.bin
```

Verification: `strings bl32-usbinit.bin | grep USB_INIT_HALT` must match.

**2. flasher-hp.bin** — M55_HP XMODEM receiver that writes to OSPI.

```bash
# Build with CMSIS Toolbox (source: alif_usb-to-ospi-flasher repo)
cd /path/to/alif_usb-to-ospi-flasher
cbuild alif.csolution.yml --context flasher.release+DevKit-E7-HP --toolchain GCC
cp out/flasher/DevKit-E7-HP/release/flasher.bin \
   tools/setools/build/images/flasher-hp.bin
```

Requires: CMSIS-Toolbox, arm-none-eabi-gcc, Alif CMSIS packs (AlifSemiconductor::Ensemble@2.1.0, ThreadX@2.0.0, ARM::CMSIS@6.1.0).

**3. ATOC config** — `build/config/linux-boot-e7-ospi-usbflash.json` (already in setools).

### Full Cycle (~5 min)

```
# 1. Generate programming mode ATOC
gen_toc(config="build/config/linux-boot-e7-ospi-usbflash.json")

# 2. Flash programming mode ATOC + binaries to MRAM (~1s)
jlink_flash(config="build/config/linux-boot-e7-ospi-usbflash.json")
#    SE boots: A32 enables USB PHY, M55_HP starts XMODEM receiver
#    CDC-ACM device enumerates at /dev/cu.usbmodem12001

# 3. Transfer OSPI image via XMODEM (~4 min for 12MB)
ospi_program_usb(image="/path/to/ospi-combined.bin")

# 4. Restore normal boot ATOC
gen_toc(config="build/config/linux-boot-e7-mram.json")
jlink_flash(config="build/config/linux-boot-e7-mram.json")

# 5. Power cycle → Linux boots from OSPI
```

**CRITICAL: Always `gen_toc` before `jlink_flash`** — AppTocPackage.bin is shared; switching configs without regenerating causes the SE to boot the wrong firmware.

### Split Architecture

The flasher uses a two-core split to avoid an SE deadlock:
- **A32/TF-A** (`bl32-usbinit.bin`): Calls SE AIPM service to enable USB PHY power domain (safe from A32), then parks in WFE
- **M55_HP** (`flasher-hp.bin`): Direct register writes for USB clocks, runs XMODEM receiver

Calling `SERVICES_set_run_cfg(USB_PHY_MASK)` from M55_HP hangs the SE with no recovery except maintenance erase + power cycle.

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
