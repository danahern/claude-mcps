"""MCP server for Alif E7 flash — SE-UART ISP and J-Link (MRAM + OSPI)."""

import json
import logging
import os
import traceback

from mcp.server import Server
from mcp.types import TextContent, Tool

logger = logging.getLogger(__name__)


TOOLS = [
    Tool(
        name="list_ports",
        description="List available SE-UART serial ports (/dev/cu.usbmodem*).",
        inputSchema={
            "type": "object",
            "properties": {},
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
            },
        },
    ),
    Tool(
        name="maintenance",
        description="Enter maintenance mode: START_ISP -> SET_MAINTENANCE -> RESET -> verify. Required before flashing. Use jlink_reset=true to avoid manual power cycle.",
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
            },
            "required": ["config"],
        },
    ),
    Tool(
        name="flash",
        description="Write ATOC package + all images to MRAM from an ATOC JSON config. Writes AppTocPackage.bin first, then all config entries with mramAddress+binary fields. Board must be in maintenance mode.",
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
            },
        },
    ),
    Tool(
        name="jlink_setup",
        description="Check or install J-Link device definition for Alif E7 (MRAM + OSPI). Reports OSPI flash loader status. Run once before using jlink_flash.",
        inputSchema={
            "type": "object",
            "properties": {
                "install": {
                    "type": "boolean",
                    "description": "Install device definition if not present (default: false, just check)",
                    "default": False,
                },
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
            },
        },
    ),
    Tool(
        name="monitor",
        description="Read serial console output at a given baud rate. Use after moving J15 jumper to UART2 position. Can optionally wait for a board power cycle (unplug/replug) to capture boot output from the start.",
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
                "wait_for_replug": {
                    "type": "boolean",
                    "description": "Wait for board unplug/replug before reading (captures boot output)",
                    "default": False,
                },
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


def _ospi_program_single(data: bytes, addr: int, verify: bool) -> dict:
    """Program a single image via RTT."""
    from . import ospi_rtt
    import pylink
    import time

    jlink = pylink.JLink()
    try:
        jlink.open()
        jlink.connect(ospi_rtt.DEVICE, verbose=True)
        jlink.rtt_start()
        time.sleep(0.5)  # Wait for RTT control block

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
            result = await asyncio.to_thread(
                isp.enter_maintenance, port,
                do_wait_for_replug=wait_replug, jlink_reset=jlink_reset
            )
            return _json(result)

        case "gen_toc":
            if not setools_dir:
                return _text("Error: --setools-dir not configured")
            config_rel = args["config"]
            result = await asyncio.to_thread(isp.gen_toc, setools_dir, config_rel)
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
            result = await asyncio.to_thread(
                isp.flash_images, port, config_path, enter_maint,
                do_wait_for_replug=wait_replug, jlink_reset=jlink_reset
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
                    jlink.flash_from_config, config, verify, erase)
            else:
                image_dir = args.get("image_dir", "")
                if not image_dir:
                    return _text("Error: provide either 'image_dir' or 'config'")
                components = args.get("components")
                result = await asyncio.to_thread(
                    jlink.flash_images, image_dir, components, verify, erase)
            return _json(result)

        case "jlink_setup":
            from . import jlink
            if args.get("install", False):
                result = await asyncio.to_thread(jlink.install_device_def)
            else:
                result = jlink.check_setup()
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
                    ospi_rtt.connect_and_program, config, verify)
            elif image and address:
                addr = int(address, 16) if isinstance(address, str) else address
                with open(image, "rb") as f:
                    data = f.read()
                result = await asyncio.to_thread(
                    _ospi_program_single, data, addr, verify)
            else:
                return _text("Error: provide 'config' or both 'image' and 'address'")
            return _json(result)

        case "monitor":
            port = _resolve_port(args)
            baud = args.get("baud", 115200)
            duration = args.get("duration", 15)
            wait_replug = args.get("wait_for_replug", False)
            result = await asyncio.to_thread(
                isp.monitor, port, baud, duration, wait_replug
            )
            return _json(result)

        case _:
            return _text(f"Unknown tool: {name}")
