"""MCP server for Alif Ensemble flash — SE-UART ISP and J-Link (MRAM + OSPI)."""

import json
import logging
import os
import traceback

from mcp.server import Server
from mcp.types import TextContent, Tool

logger = logging.getLogger(__name__)

_DEVICE_PROPERTY = {
    "type": "string",
    "description": "Target device (default: alif-e7). Available: alif-e7, alif-e8.",
    "default": "alif-e7",
}

TOOLS = [
    Tool(
        name="list_ports",
        description="List available SE-UART serial ports (/dev/cu.usbmodem*).",
        inputSchema={
            "type": "object",
            "properties": {
                "device": _DEVICE_PROPERTY,
            },
        },
    ),
    Tool(
        name="probe",
        description="Check if SE-UART is responsive and report ISP/maintenance mode status.",
        inputSchema={
            "type": "object",
            "properties": {
                "port": {
                    "type": "string",
                    "description": "Serial port path. Auto-detected if omitted.",
                },
                "device": _DEVICE_PROPERTY,
            },
        },
    ),
    Tool(
        name="maintenance",
        description=(
            "Enter maintenance mode: START_ISP -> SET_MAINTENANCE -> RESET -> verify. "
            "Required before flashing. "
            "Use wait_for_power_cycle=true when the port stays alive across power cycles "
            "(FTDI adapters): call this FIRST, then immediately power-cycle the board — "
            "the tool polls START_ISP for power_cycle_timeout seconds while you do it."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "port": {
                    "type": "string",
                    "description": "Serial port path. Auto-detected if omitted.",
                },
                "jlink_reset": {
                    "type": "boolean",
                    "description": "Reset board via JLink before entering maintenance (no manual power cycle needed)",
                    "default": False,
                },
                "wait_for_replug": {
                    "type": "boolean",
                    "description": "Wait for manual USB unplug/replug before sending ISP commands",
                    "default": False,
                },
                "wait_for_power_cycle": {
                    "type": "boolean",
                    "description": (
                        "Poll START_ISP for power_cycle_timeout seconds — start this call BEFORE "
                        "power-cycling the board. Use when FTDI port stays present across power cycles."
                    ),
                    "default": False,
                },
                "power_cycle_timeout": {
                    "type": "number",
                    "description": "Seconds to poll for SE response when wait_for_power_cycle=true (default: 15)",
                    "default": 15,
                },
                "device": _DEVICE_PROPERTY,
            },
        },
    ),
    Tool(
        name="gen_toc",
        description="Run app-gen-toc to generate ATOC package from a JSON config file.",
        inputSchema={
            "type": "object",
            "properties": {
                "config": {
                    "type": "string",
                    "description": "Config path relative to setools_dir (e.g. 'build/config/linux-boot-e7.json')",
                },
                "device": _DEVICE_PROPERTY,
            },
            "required": ["config"],
        },
    ),
    Tool(
        name="flash",
        description=(
            "Write ATOC package + all images to MRAM from an ATOC JSON config. "
            "Writes AppTocPackage.bin first, then all config entries with mramAddress+binary fields. "
            "Board must be in maintenance mode. "
            "Use maintenance=true + wait_for_power_cycle=true to enter maintenance automatically: "
            "call flash first, then power-cycle the board while it polls."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "config": {
                    "type": "string",
                    "description": "Absolute path to ATOC JSON config, or relative to setools_dir/build/config/",
                },
                "port": {
                    "type": "string",
                    "description": "Serial port path. Auto-detected if omitted.",
                },
                "maintenance": {
                    "type": "boolean",
                    "description": "Enter maintenance mode first (default: false)",
                    "default": False,
                },
                "jlink_reset": {
                    "type": "boolean",
                    "description": "Reset board via JLink before entering maintenance (no manual power cycle needed). Requires maintenance=true.",
                    "default": False,
                },
                "wait_for_replug": {
                    "type": "boolean",
                    "description": "Wait for manual USB unplug/replug before entering maintenance. Requires maintenance=true.",
                    "default": False,
                },
                "wait_for_power_cycle": {
                    "type": "boolean",
                    "description": (
                        "Poll START_ISP for power_cycle_timeout seconds — start this call BEFORE "
                        "power-cycling the board. Requires maintenance=true."
                    ),
                    "default": False,
                },
                "power_cycle_timeout": {
                    "type": "number",
                    "description": "Seconds to poll for SE response when wait_for_power_cycle=true (default: 15)",
                    "default": 15,
                },
                "device": _DEVICE_PROPERTY,
            },
            "required": ["config"],
        },
    ),
    Tool(
        name="jlink_flash",
        description="Flash images to MRAM or OSPI via J-Link loadbin. MRAM: ~44 KB/s direct write. OSPI: uses flash loader (slower, erase cycles). Config entries use 'address', 'mramAddress', or 'ospiAddress' fields. Board must be freshly power-cycled. Auto-installs JLink device definition if needed.",
        inputSchema={
            "type": "object",
            "properties": {
                "image_dir": {
                    "type": "string",
                    "description": "Directory containing image files. If using config, this is auto-resolved.",
                },
                "config": {
                    "type": "string",
                    "description": "ATOC JSON config path (alternative to image_dir — extracts files and addresses from config).",
                },
                "components": {
                    "type": "array",
                    "items": {"type": "string", "enum": ["tfa", "dtb", "kernel", "rootfs"]},
                    "description": "Which components to flash (default: all). Only used with image_dir.",
                },
                "verify": {
                    "type": "boolean",
                    "description": "Verify after programming (default: false)",
                    "default": False,
                },
                "erase": {
                    "type": "boolean",
                    "description": "Pre-erase OSPI region before programming. Clears stale data that kernel MTD partition parsers might misinterpret. Only affects OSPI addresses. (default: false)",
                    "default": False,
                },
                "device": _DEVICE_PROPERTY,
            },
        },
    ),
    Tool(
        name="jlink_setup",
        description="Check or install J-Link device definition for Alif Ensemble (MRAM + OSPI). Reports OSPI flash loader status. Run once before using jlink_flash.",
        inputSchema={
            "type": "object",
            "properties": {
                "install": {
                    "type": "boolean",
                    "description": "Install device definition if not present (default: false, just check)",
                    "default": False,
                },
                "device": _DEVICE_PROPERTY,
            },
        },
    ),
    Tool(
        name="ospi_program",
        description="BROKEN: M55_HP BusFault on OSPI controller access. Use jlink_flash instead (~7 KB/s OSPI via FLM).",
        inputSchema={
            "type": "object",
            "properties": {
                "config": {
                    "type": "string",
                    "description": "ATOC JSON config with OSPI addresses (entries with 'address' >= 0xC0000000).",
                },
                "image": {
                    "type": "string",
                    "description": "Single image file path (alternative to config).",
                },
                "address": {
                    "type": "string",
                    "description": "OSPI address for single image (e.g. '0xC0000000'). Required with 'image'.",
                },
                "verify": {
                    "type": "boolean",
                    "description": "Verify CRC32 after programming (default: true)",
                    "default": True,
                },
                "device": _DEVICE_PROPERTY,
            },
        },
    ),
    Tool(
        name="ospi_program_usb",
        description=(
            "Program OSPI flash via USB CDC-ACM XMODEM transfer. "
            "Auto-detects Alif CDC-ACM device (VID 0x0525), sends binary via "
            "XMODEM-CRC (128-byte blocks), and waits for flasher confirmation. "
            "Four timeout layers: receiver ready (30s), per-block ACK (10s), "
            "post-EOT completion (30s), overall wall clock (auto-calculated "
            "from file size as (size/30KB)*2)."
        ),
        inputSchema={
            "type": "object",
            "properties": {
                "image": {
                    "type": "string",
                    "description": "Path to binary image file (e.g., combined OSPI image)",
                },
                "device": {
                    "type": "string",
                    "description": "Serial device path (e.g., /dev/cu.usbmodem12001). Auto-detected if omitted.",
                },
                "timeout": {
                    "type": "number",
                    "description": "Max transfer time in seconds. Default: auto-calculated from file size.",
                },
            },
            "required": ["image"],
        },
    ),
    Tool(
        name="monitor",
        description="Read serial console output at a given baud rate. Use jlink_reset=true to trigger a JLink NSRST reset and capture SE boot output (e.g. to check for '[SES] No ATOC' or boot success). Can also wait for a board power cycle (unplug/replug) to capture boot output from the start.",
        inputSchema={
            "type": "object",
            "properties": {
                "port": {
                    "type": "string",
                    "description": "Serial port path. Auto-detected if omitted.",
                },
                "baud": {
                    "type": "integer",
                    "description": "Baud rate (default: 115200)",
                    "default": 115200,
                },
                "duration": {
                    "type": "number",
                    "description": "How long to read in seconds (default: 15)",
                    "default": 15,
                },
                "jlink_reset": {
                    "type": "boolean",
                    "description": "Trigger JLink NSRST reset before reading — captures SE boot output from the start. Port is opened first, then reset fires.",
                    "default": False,
                },
                "wait_for_replug": {
                    "type": "boolean",
                    "description": "Wait for board unplug/replug before reading (captures boot output)",
                    "default": False,
                },
                "device": _DEVICE_PROPERTY,
            },
        },
    ),
]


def _text(content: str) -> list[TextContent]:
    return [TextContent(type="text", text=content)]


def _json(data) -> list[TextContent]:
    return [TextContent(type="text", text=json.dumps(data, indent=2))]


def _resolve_port(args: dict) -> str:
    """Resolve serial port from args or auto-detect.

    Prefers usbserial (FTDI) over usbmodem (JLink VCOM) since the
    SE-UART ISP protocol runs on the FTDI adapter.
    """
    from . import isp

    port = args.get("port")
    if port:
        return port
    ports = isp.find_se_uart()
    if not ports:
        raise RuntimeError("No SE-UART ports found. Is PRG_USB connected?")
    # Prefer FTDI (usbserial) over JLink VCOM (usbmodem)
    ftdi = [p for p in ports if "usbserial" in p]
    return ftdi[0] if ftdi else ports[0]


def _ospi_program_single(data: bytes, addr: int, verify: bool,
                         device: str | None = None) -> dict:
    """Program a single image via RTT."""
    from . import devices
    import pylink
    import time

    cfg = devices.get_config(device)
    jlink = pylink.JLink()
    try:
        jlink.open()
        jlink.connect(cfg["jlink_device"], verbose=True)
        jlink.rtt_start()
        time.sleep(0.5)  # Wait for RTT control block

        from . import ospi_rtt
        programmer = ospi_rtt.OspiProgrammer(jlink)
        version = programmer.ping()
        result = programmer.flash_image(addr, data, verify=verify)
        result["firmware_version"] = version
        return result
    finally:
        try:
            jlink.rtt_stop()
        except Exception:
            pass
        jlink.close()


def _ospi_program_usb(image: str, device: str,
                      timeout_override: float | None = None) -> dict:
    """Run XMODEM transfer over USB CDC-ACM. Called from thread."""
    from . import xmodem
    import serial as _serial

    port = _serial.Serial(device, 115200, timeout=1)
    try:
        result = xmodem.xmodem_send(port, image)
        result["device"] = device
        return result
    finally:
        port.close()


def create_server(setools_dir: str | None = None) -> Server:
    server = Server("alif-flash")
    _setools_dir = setools_dir

    @server.list_tools()
    async def list_tools() -> list[Tool]:
        return TOOLS

    @server.call_tool()
    async def call_tool(name: str, arguments: dict) -> list[TextContent]:
        try:
            return await _dispatch(name, arguments, _setools_dir)
        except Exception as e:
            logger.exception("Tool %s failed", name)
            return _text(f"Error: {e}\n\n{traceback.format_exc()}")

    return server


async def _dispatch(name: str, args: dict, setools_dir: str | None) -> list[TextContent]:
    import asyncio
    from . import isp

    device = args.get("device")

    match name:
        case "list_ports":
            ports = isp.find_se_uart()
            return _json({"ports": ports, "count": len(ports)})

        case "probe":
            port = _resolve_port(args)
            result = await asyncio.to_thread(isp.probe, port)
            return _json(result)

        case "maintenance":
            port = _resolve_port(args)
            jlink_reset = args.get("jlink_reset", False)
            wait_replug = args.get("wait_for_replug", False)
            wait_power_cycle = args.get("wait_for_power_cycle", False)
            power_cycle_timeout = float(args.get("power_cycle_timeout", 15))
            result = await asyncio.to_thread(
                isp.enter_maintenance, port,
                do_wait_for_replug=wait_replug, jlink_reset=jlink_reset,
                wait_for_power_cycle=wait_power_cycle,
                power_cycle_timeout=power_cycle_timeout,
            )
            return _json(result)

        case "gen_toc":
            if not setools_dir:
                return _text("Error: --setools-dir not configured")
            config_rel = args["config"]
            result = await asyncio.to_thread(
                isp.gen_toc, setools_dir, config_rel, device=device)
            return _json(result)

        case "flash":
            port = _resolve_port(args)
            config_path = args["config"]
            # Resolve relative config paths against setools_dir
            if not os.path.isabs(config_path) and setools_dir:
                config_path = os.path.join(setools_dir, config_path)
            enter_maint = args.get("maintenance", False)
            jlink_reset = args.get("jlink_reset", False)
            wait_replug = args.get("wait_for_replug", False)
            wait_power_cycle = args.get("wait_for_power_cycle", False)
            power_cycle_timeout = float(args.get("power_cycle_timeout", 15))
            result = await asyncio.to_thread(
                isp.flash_images, port, config_path, enter_maint,
                do_wait_for_replug=wait_replug, jlink_reset=jlink_reset,
                wait_for_power_cycle=wait_power_cycle,
                power_cycle_timeout=power_cycle_timeout,
                device=device
            )
            return _json(result)

        case "jlink_flash":
            from . import jlink
            config = args.get("config")
            verify = args.get("verify", False)
            erase = args.get("erase", False)
            if config:
                if not os.path.isabs(config) and setools_dir:
                    config = os.path.join(setools_dir, config)
                result = await asyncio.to_thread(
                    jlink.flash_from_config, config, verify, erase,
                    device=device)
            else:
                image_dir = args.get("image_dir", "")
                if not image_dir:
                    return _text("Error: provide either 'image_dir' or 'config'")
                components = args.get("components")
                result = await asyncio.to_thread(
                    jlink.flash_images, image_dir, components, verify, erase,
                    device=device)
            return _json(result)

        case "jlink_setup":
            from . import jlink
            if args.get("install", False):
                result = await asyncio.to_thread(
                    jlink.install_device_def, device=device)
            else:
                result = jlink.check_setup(device=device)
            return _json(result)

        case "ospi_program":
            from . import ospi_rtt
            config = args.get("config")
            image = args.get("image")
            address = args.get("address")
            verify = args.get("verify", True)
            if config:
                if not os.path.isabs(config) and setools_dir:
                    config = os.path.join(setools_dir, config)
                result = await asyncio.to_thread(
                    ospi_rtt.connect_and_program, config, verify,
                    device=device)
            elif image and address:
                addr = int(address, 16) if isinstance(address, str) else address
                with open(image, "rb") as f:
                    data = f.read()
                result = await asyncio.to_thread(
                    _ospi_program_single, data, addr, verify,
                    device=device)
            else:
                return _text("Error: provide 'config' or both 'image' and 'address'")
            return _json(result)

        case "ospi_program_usb":
            from . import xmodem
            image = args.get("image", "")
            if not image:
                return _text("Error: 'image' parameter is required")
            if not os.path.isfile(image):
                return _text(f"Error: file not found: {image}")
            usb_device = args.get("device", "")
            if not usb_device:
                usb_device = xmodem.find_cdc_device()
                if not usb_device:
                    return _text(
                        "Error: No Alif CDC-ACM device found — "
                        "is programming mode ATOC flashed and J2 connected?"
                    )
            timeout_override = args.get("timeout")
            result = await asyncio.to_thread(
                _ospi_program_usb, image, usb_device, timeout_override)
            return _json(result)

        case "monitor":
            port = _resolve_port(args)
            baud = args.get("baud", 115200)
            duration = args.get("duration", 15)
            wait_replug = args.get("wait_for_replug", False)
            jlink_reset = args.get("jlink_reset", False)
            result = await asyncio.to_thread(
                isp.monitor, port, baud, duration, wait_replug,
                jlink_reset,
            )
            return _json(result)

        case _:
            return _text(f"Unknown tool: {name}")
