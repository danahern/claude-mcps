"""MCP server for bidirectional UART communication — session-based serial console."""

import asyncio
import json
import logging
import traceback

from mcp.server import Server
from mcp.types import TextContent, Tool

logger = logging.getLogger(__name__)

TOOLS = [
    Tool(
        name="list_ports",
        description="List available serial ports with metadata (device path, description, manufacturer).",
        inputSchema={
            "type": "object",
            "properties": {},
        },
    ),
    Tool(
        name="open_port",
        description="Open a serial port session. Returns a session_id for subsequent commands.",
        inputSchema={
            "type": "object",
            "properties": {
                "port": {
                    "type": "string",
                    "description": "Serial port path (e.g. /dev/cu.usbserial-AO009AHE)",
                },
                "baud": {
                    "type": "integer",
                    "description": "Baud rate (default: 115200)",
                    "default": 115200,
                },
                "echo_filter": {
                    "type": "boolean",
                    "description": "Strip echoed commands from output (default: true)",
                    "default": True,
                },
            },
            "required": ["port"],
        },
    ),
    Tool(
        name="close_port",
        description="Close a serial session and release the port.",
        inputSchema={
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session ID from open_port",
                },
            },
            "required": ["session_id"],
        },
    ),
    Tool(
        name="send_command",
        description="Send a command string to the serial port and wait for a response. Appends \\r\\n automatically. Uses idle timeout to detect end of response, or regex prompt matching for faster detection.",
        inputSchema={
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session ID from open_port",
                },
                "command": {
                    "type": "string",
                    "description": "Command to send",
                },
                "timeout": {
                    "type": "number",
                    "description": "Idle timeout in seconds — stop after no new data for this long (default: 0.5)",
                    "default": 0.5,
                },
                "wait_for": {
                    "type": "string",
                    "description": "Regex pattern to match in output — stops reading immediately on match (e.g. '[#$] $' for shell prompt, '=> $' for U-Boot)",
                },
            },
            "required": ["session_id", "command"],
        },
    ),
    Tool(
        name="read_output",
        description="Read any pending output from the serial port (non-blocking drain). Use to check for unsolicited output or after write_raw.",
        inputSchema={
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session ID from open_port",
                },
                "timeout": {
                    "type": "number",
                    "description": "Idle timeout in seconds (default: 0.5)",
                    "default": 0.5,
                },
            },
            "required": ["session_id"],
        },
    ),
    Tool(
        name="write_raw",
        description="Write raw bytes to the serial port without waiting for a response. Use for binary protocols or control characters.",
        inputSchema={
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session ID from open_port",
                },
                "data": {
                    "type": "string",
                    "description": "Data to write. Interpreted as UTF-8 text unless hex=true.",
                },
                "hex": {
                    "type": "boolean",
                    "description": "If true, interpret data as hex string (e.g. '0d0a' for \\r\\n)",
                    "default": False,
                },
            },
            "required": ["session_id", "data"],
        },
    ),
]


def _text(content: str) -> list[TextContent]:
    return [TextContent(type="text", text=content)]


def _json(data) -> list[TextContent]:
    return [TextContent(type="text", text=json.dumps(data, indent=2))]


def create_server() -> Server:
    server = Server("uart-mcp")
    sessions: dict = {}

    @server.list_tools()
    async def list_tools() -> list[Tool]:
        return TOOLS

    @server.call_tool()
    async def call_tool(name: str, arguments: dict) -> list[TextContent]:
        try:
            return await _dispatch(name, arguments, sessions)
        except Exception as e:
            logger.exception("Tool %s failed", name)
            return _text(f"Error: {e}\n\n{traceback.format_exc()}")

    return server


async def _dispatch(name: str, args: dict, sessions: dict) -> list[TextContent]:
    from .serial_session import SerialSession, list_serial_ports

    match name:
        case "list_ports":
            ports = await asyncio.to_thread(list_serial_ports)
            return _json({"ports": ports, "count": len(ports)})

        case "open_port":
            port = args["port"]
            baud = args.get("baud", 115200)
            echo_filter = args.get("echo_filter", True)
            session = SerialSession(
                port=port,
                baud=baud,
                echo_filter=echo_filter,
            )
            await asyncio.to_thread(session.open)
            sessions[session.session_id] = session
            return _json({
                "session_id": session.session_id,
                "port": port,
                "baud": baud,
                "status": "open",
            })

        case "close_port":
            session = _get_session(sessions, args["session_id"])
            await asyncio.to_thread(session.close)
            del sessions[session.session_id]
            return _json({
                "session_id": session.session_id,
                "status": "closed",
            })

        case "send_command":
            session = _get_session(sessions, args["session_id"])
            command = args["command"]
            timeout = args.get("timeout", 0.5)
            wait_for = args.get("wait_for")
            output = await asyncio.to_thread(
                session.send_command, command, timeout, wait_for
            )
            return _json({
                "session_id": session.session_id,
                "command": command,
                "output": output,
            })

        case "read_output":
            session = _get_session(sessions, args["session_id"])
            timeout = args.get("timeout", 0.5)
            output = await asyncio.to_thread(session.read_output, timeout)
            return _json({
                "session_id": session.session_id,
                "output": output,
            })

        case "write_raw":
            session = _get_session(sessions, args["session_id"])
            data_str = args["data"]
            if args.get("hex", False):
                data = bytes.fromhex(data_str)
            else:
                data = data_str.encode("utf-8")
            bytes_written = await asyncio.to_thread(session.write_raw, data)
            return _json({
                "session_id": session.session_id,
                "bytes_written": bytes_written,
            })

        case _:
            return _text(f"Unknown tool: {name}")


def _get_session(sessions: dict, session_id: str):
    """Look up a session by ID, raising a clear error if not found."""
    from .serial_session import SerialSession

    session: SerialSession | None = sessions.get(session_id)
    if session is None:
        active = list(sessions.keys())
        raise ValueError(
            f"No session with id '{session_id}'. "
            f"Active sessions: {active if active else 'none — use open_port first'}"
        )
    return session
