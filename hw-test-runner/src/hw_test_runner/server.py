"""MCP server for hardware testing — BLE and TCP operations."""

import json
import logging
import traceback

from mcp.server import Server
from mcp.types import TextContent, Tool

logger = logging.getLogger(__name__)


TOOLS = [
    # ---- Low-level BLE ----
    Tool(
        name="ble_discover",
        description="Scan for BLE devices. Optionally filter by advertised service UUID.",
        inputSchema={
            "type": "object",
            "properties": {
                "service_uuid": {
                    "type": "string",
                    "description": "Filter by service UUID (optional)",
                },
                "timeout": {
                    "type": "number",
                    "description": "Scan timeout in seconds (default: 5)",
                    "default": 5,
                },
            },
        },
    ),
    Tool(
        name="ble_read",
        description="Connect to a BLE device, read a GATT characteristic, disconnect.",
        inputSchema={
            "type": "object",
            "properties": {
                "address": {
                    "type": "string",
                    "description": "BLE device address (UUID on macOS)",
                },
                "characteristic_uuid": {
                    "type": "string",
                    "description": "GATT characteristic UUID to read",
                },
            },
            "required": ["address", "characteristic_uuid"],
        },
    ),
    Tool(
        name="ble_write",
        description="Connect to a BLE device, write a GATT characteristic, disconnect.",
        inputSchema={
            "type": "object",
            "properties": {
                "address": {
                    "type": "string",
                    "description": "BLE device address",
                },
                "characteristic_uuid": {
                    "type": "string",
                    "description": "GATT characteristic UUID to write",
                },
                "data": {
                    "type": "string",
                    "description": "Hex-encoded data to write (e.g., '0102ff')",
                },
            },
            "required": ["address", "characteristic_uuid", "data"],
        },
    ),
    Tool(
        name="ble_subscribe",
        description="Subscribe to BLE notifications and collect data for a timeout period.",
        inputSchema={
            "type": "object",
            "properties": {
                "address": {
                    "type": "string",
                    "description": "BLE device address",
                },
                "characteristic_uuid": {
                    "type": "string",
                    "description": "GATT characteristic UUID to subscribe",
                },
                "timeout": {
                    "type": "number",
                    "description": "Collection timeout in seconds (default: 10)",
                    "default": 10,
                },
            },
            "required": ["address", "characteristic_uuid"],
        },
    ),
    # ---- WiFi Provisioning (high-level) ----
    Tool(
        name="wifi_provision",
        description="Full WiFi provisioning flow over BLE: discover device, send credentials, wait for connection.",
        inputSchema={
            "type": "object",
            "properties": {
                "ssid": {"type": "string", "description": "WiFi network SSID"},
                "psk": {"type": "string", "description": "WiFi password"},
                "security": {
                    "type": "string",
                    "description": "Security type: Open, WPA-PSK, WPA2-PSK (default), WPA3-SAE",
                },
                "address": {
                    "type": "string",
                    "description": "BLE device address (auto-discover if omitted)",
                },
                "timeout": {
                    "type": "number",
                    "description": "Timeout in seconds (default: 30)",
                    "default": 30,
                },
            },
            "required": ["ssid", "psk"],
        },
    ),
    Tool(
        name="wifi_scan_aps",
        description="Trigger a WiFi AP scan on the device and return discovered access points.",
        inputSchema={
            "type": "object",
            "properties": {
                "address": {
                    "type": "string",
                    "description": "BLE device address (auto-discover if omitted)",
                },
                "timeout": {
                    "type": "number",
                    "description": "Timeout in seconds (default: 15)",
                    "default": 15,
                },
            },
        },
    ),
    Tool(
        name="wifi_status",
        description="Query WiFi connection status via BLE.",
        inputSchema={
            "type": "object",
            "properties": {
                "address": {
                    "type": "string",
                    "description": "BLE device address (auto-discover if omitted)",
                },
            },
        },
    ),
    Tool(
        name="wifi_factory_reset",
        description="Send factory reset command to clear stored WiFi credentials.",
        inputSchema={
            "type": "object",
            "properties": {
                "address": {
                    "type": "string",
                    "description": "BLE device address (auto-discover if omitted)",
                },
            },
        },
    ),
    # ---- TCP Throughput ----
    Tool(
        name="tcp_throughput",
        description="Run a TCP throughput test (upload, download, or echo) against a device.",
        inputSchema={
            "type": "object",
            "properties": {
                "host": {"type": "string", "description": "Target IP address"},
                "mode": {
                    "type": "string",
                    "description": "Test mode: upload, download, or echo",
                    "enum": ["upload", "download", "echo"],
                },
                "port": {
                    "type": "integer",
                    "description": "TCP port (default: 4242)",
                    "default": 4242,
                },
                "duration": {
                    "type": "number",
                    "description": "Test duration in seconds (default: 10)",
                    "default": 10,
                },
                "block_size": {
                    "type": "integer",
                    "description": "Data block size in bytes (default: 1024)",
                    "default": 1024,
                },
            },
            "required": ["host", "mode"],
        },
    ),
]


def _text(content: str) -> list[TextContent]:
    return [TextContent(type="text", text=content)]


def _json(data) -> list[TextContent]:
    return [TextContent(type="text", text=json.dumps(data, indent=2))]


def create_server() -> Server:
    server = Server("hw-test-runner")

    @server.list_tools()
    async def list_tools() -> list[Tool]:
        return TOOLS

    @server.call_tool()
    async def call_tool(name: str, arguments: dict) -> list[TextContent]:
        try:
            return await _dispatch(name, arguments)
        except Exception as e:
            logger.exception("Tool %s failed", name)
            return _text(f"Error: {e}\n\n{traceback.format_exc()}")

    return server


async def _dispatch(name: str, args: dict) -> list[TextContent]:
    from . import ble, provisioning, throughput

    match name:
        # Low-level BLE
        case "ble_discover":
            results = await ble.discover(
                service_uuid=args.get("service_uuid"),
                timeout=args.get("timeout", 5.0),
            )
            return _json({"devices": results, "count": len(results)})

        case "ble_read":
            data = await ble.read_characteristic(
                args["address"], args["characteristic_uuid"]
            )
            return _json({"hex": data.hex(), "length": len(data)})

        case "ble_write":
            data = bytes.fromhex(args["data"])
            await ble.write_characteristic(
                args["address"], args["characteristic_uuid"], data
            )
            return _json({"success": True, "bytes_written": len(data)})

        case "ble_subscribe":
            notifications = await ble.subscribe_notifications(
                args["address"],
                args["characteristic_uuid"],
                timeout=args.get("timeout", 10.0),
            )
            return _json({
                "count": len(notifications),
                "data": [n.hex() for n in notifications],
            })

        # WiFi Provisioning
        case "wifi_provision":
            result = await provisioning.provision(
                ssid=args["ssid"],
                psk=args["psk"],
                security=args.get("security"),
                address=args.get("address"),
                timeout=args.get("timeout", 30.0),
            )
            return _json(result)

        case "wifi_scan_aps":
            aps = await provisioning.scan_aps(
                address=args.get("address"),
                timeout=args.get("timeout", 15.0),
            )
            return _json({"access_points": aps, "count": len(aps)})

        case "wifi_status":
            status = await provisioning.get_status(
                address=args.get("address"),
            )
            return _json(status)

        case "wifi_factory_reset":
            result = await provisioning.factory_reset(
                address=args.get("address"),
            )
            return _json(result)

        # TCP Throughput
        case "tcp_throughput":
            result = await throughput.tcp_throughput(
                host=args["host"],
                mode=args["mode"],
                port=args.get("port", 4242),
                duration=args.get("duration", 10.0),
                block_size=args.get("block_size", 1024),
            )
            return _json(result)

        case _:
            return _text(f"Unknown tool: {name}")
