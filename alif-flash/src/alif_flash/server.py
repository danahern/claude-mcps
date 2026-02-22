"""MCP server for Alif E7 MRAM flash — SE-UART ISP protocol."""

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
        description="Write ATOC package + all images to MRAM from an ATOC JSON config. Writes AppTocPackage.bin to 0x80000000 first, then TFA/DTB/KERNEL/ROOTFS. Board must be in maintenance mode.",
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
