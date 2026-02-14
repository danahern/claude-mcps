import csv
import io
import logging
import os
import uuid
from datetime import datetime
from typing import Any

from mcp.server import Server
from mcp.types import TextContent, Tool

from .analysis import (
    analyze_i2c_data,
    analyze_spi_data,
    analyze_uart_data,
    compute_timing_info,
    deep_analyze_analog,
    deep_analyze_digital,
    deep_analyze_protocol,
    search_csv_data,
)
from .config import Config

logger = logging.getLogger(__name__)

# Lazy import — logic2-automation may not be installed in test environments
automation = None

# Known valid sample rate pairs per device type (from Saleae specs).
# No runtime API to query these — must be hardcoded.
DEVICE_RATE_INFO = {
    "LOGIC_PRO_16": {
        "max_digital": 500_000_000,
        "max_analog": 50_000_000,
        "has_analog": True,
        "channels": 16,
        "suggested_pairs": [
            (125_000_000, 12_500_000),
            (50_000_000, 12_500_000),
            (50_000_000, 6_250_000),
            (25_000_000, 3_125_000),
        ],
    },
    "LOGIC_PRO_8": {
        "max_digital": 500_000_000,
        "max_analog": 50_000_000,
        "has_analog": True,
        "channels": 8,
        "suggested_pairs": [
            (125_000_000, 12_500_000),
            (50_000_000, 6_250_000),
            (25_000_000, 3_125_000),
        ],
    },
    "LOGIC_8": {
        "max_digital": 100_000_000,
        "max_analog": 0,
        "has_analog": False,
        "channels": 8,
        "suggested_pairs": [],
    },
}

# Default rates when analog channels are requested but no rates specified
_DEFAULT_DIGITAL_ANALOG_RATE = (50_000_000, 6_250_000)


def _get_automation():
    global automation
    if automation is None:
        from saleae import automation as _automation
        automation = _automation
    return automation


def create_server(config: Config) -> Server:
    server = Server("saleae-logic")

    # State: gRPC manager connection, active captures, analyzer handles
    state: dict[str, Any] = {
        "config": config,
        "manager": None,
        "captures": {},      # capture_id -> Capture object
        "analyzers": {},     # capture_id -> {analyzer_index: AnalyzerHandle}
        "next_analyzer": 0,  # global analyzer index counter
    }

    def _ensure_output_dir():
        os.makedirs(config.output_dir, exist_ok=True)

    def _get_manager():
        auto = _get_automation()
        if state["manager"] is None:
            state["manager"] = auto.Manager.connect(
                address=config.host, port=config.port
            )
        return state["manager"]

    def _store_capture(capture) -> str:
        capture_id = str(uuid.uuid4())[:8]
        state["captures"][capture_id] = capture
        state["analyzers"][capture_id] = {}
        return capture_id

    def _get_capture(capture_id: str):
        cap = state["captures"].get(capture_id)
        if cap is None:
            raise ValueError(f"Unknown capture_id: {capture_id}")
        return cap

    def _store_analyzer(capture_id: str, handle) -> int:
        idx = state["next_analyzer"]
        state["next_analyzer"] += 1
        state["analyzers"][capture_id][idx] = handle
        return idx

    def _get_analyzer(capture_id: str, analyzer_index: int):
        analyzers = state["analyzers"].get(capture_id, {})
        handle = analyzers.get(analyzer_index)
        if handle is None:
            raise ValueError(
                f"Unknown analyzer_index {analyzer_index} for capture {capture_id}"
            )
        return handle

    def _abs_path(path: str) -> str:
        """Resolve relative paths to absolute — Logic 2 is a separate process."""
        return os.path.abspath(path)

    def _text(content: str) -> list[TextContent]:
        return [TextContent(type="text", text=content)]

    def _json_text(data: Any) -> list[TextContent]:
        import json
        return [TextContent(type="text", text=json.dumps(data, indent=2))]

    # ── Tool definitions ────────────────────────────────────────────────

    TOOLS = [
        Tool(
            name="get_app_info",
            description="Get Logic 2 version and connection status",
            inputSchema={"type": "object", "properties": {}},
        ),
        Tool(
            name="list_devices",
            description="List connected Saleae analyzers (type, serial, channels)",
            inputSchema={
                "type": "object",
                "properties": {
                    "include_simulation": {
                        "type": "boolean",
                        "description": "Include simulation devices",
                        "default": False,
                    }
                },
            },
        ),
        Tool(
            name="start_capture",
            description=(
                "Start recording signals. Supports digital + analog channels, "
                "manual/timed/triggered capture modes."
            ),
            inputSchema={
                "type": "object",
                "properties": {
                    "device_id": {
                        "type": "string",
                        "description": "Device serial number (auto-select if omitted)",
                    },
                    "channels": {
                        "type": "array",
                        "items": {"type": "integer"},
                        "description": "Digital channel numbers to enable",
                    },
                    "analog_channels": {
                        "type": "array",
                        "items": {"type": "integer"},
                        "description": "Analog channel numbers to enable",
                    },
                    "sample_rate": {
                        "type": "integer",
                        "description": (
                            "Digital sample rate in Hz. Default: 10M (digital-only) or "
                            "50M (with analog). Max: 500M (Pro 16/8), 100M (Logic 8). "
                            "Must be a valid pair with analog_sample_rate when both are used."
                        ),
                    },
                    "analog_sample_rate": {
                        "type": "integer",
                        "description": (
                            "Analog sample rate in Hz. Auto-selected (6.25M) if omitted "
                            "when analog_channels are present. Valid pairs with digital rate: "
                            "125M+12.5M, 50M+12.5M, 50M+6.25M, 25M+3.125M."
                        ),
                    },
                    "duration_seconds": {
                        "type": "number",
                        "description": "Timed capture duration (omit for manual mode)",
                    },
                    "trigger_channel": {
                        "type": "integer",
                        "description": "Digital channel for trigger",
                    },
                    "trigger_type": {
                        "type": "string",
                        "enum": ["rising", "falling", "pulse_high", "pulse_low"],
                        "description": "Trigger type",
                    },
                    "after_trigger_seconds": {
                        "type": "number",
                        "description": "Seconds to capture after trigger fires",
                    },
                    "logic_level": {
                        "type": "number",
                        "description": "Voltage threshold (default: 3.3, Logic Pro only)",
                    },
                    "glitch_filter_ns": {
                        "type": "object",
                        "description": "Per-channel glitch filter in nanoseconds {channel: ns}",
                        "additionalProperties": {"type": "number"},
                    },
                },
                "required": ["channels"],
            },
        ),
        Tool(
            name="stop_capture",
            description="Stop an active manual capture",
            inputSchema={
                "type": "object",
                "properties": {
                    "capture_id": {"type": "string", "description": "Capture ID"},
                },
                "required": ["capture_id"],
            },
        ),
        Tool(
            name="wait_capture",
            description="Wait for a timed or triggered capture to complete",
            inputSchema={
                "type": "object",
                "properties": {
                    "capture_id": {"type": "string", "description": "Capture ID"},
                },
                "required": ["capture_id"],
            },
        ),
        Tool(
            name="close_capture",
            description="Close and release a capture",
            inputSchema={
                "type": "object",
                "properties": {
                    "capture_id": {"type": "string", "description": "Capture ID"},
                },
                "required": ["capture_id"],
            },
        ),
        Tool(
            name="save_capture",
            description="Save capture to .sal file",
            inputSchema={
                "type": "object",
                "properties": {
                    "capture_id": {"type": "string", "description": "Capture ID"},
                    "filepath": {
                        "type": "string",
                        "description": "Output .sal file path",
                    },
                },
                "required": ["capture_id", "filepath"],
            },
        ),
        Tool(
            name="load_capture",
            description="Load a previously saved .sal capture file",
            inputSchema={
                "type": "object",
                "properties": {
                    "filepath": {
                        "type": "string",
                        "description": "Path to .sal file",
                    },
                },
                "required": ["filepath"],
            },
        ),
        Tool(
            name="add_analyzer",
            description=(
                "Add protocol decoder (SPI, I2C, Async Serial, CAN, etc.) to a capture. "
                'Settings example for I2C: {"SDA": 0, "SCL": 1}. '
                'For SPI: {"MISO": 0, "Clock": 1, "Enable": 2}. '
                'For UART: {"Input Channel": 0, "Bit Rate": 115200}.'
            ),
            inputSchema={
                "type": "object",
                "properties": {
                    "capture_id": {"type": "string", "description": "Capture ID"},
                    "analyzer_name": {
                        "type": "string",
                        "description": 'Protocol name: "SPI", "I2C", "Async Serial", "CAN", etc.',
                    },
                    "settings": {
                        "type": "object",
                        "description": "Analyzer-specific settings (channel mappings, bit rate, etc.)",
                    },
                    "label": {
                        "type": "string",
                        "description": "Display label in Logic 2 UI",
                    },
                },
                "required": ["capture_id", "analyzer_name", "settings"],
            },
        ),
        Tool(
            name="add_high_level_analyzer",
            description="Attach a custom High-Level Analyzer (HLA) extension",
            inputSchema={
                "type": "object",
                "properties": {
                    "capture_id": {"type": "string", "description": "Capture ID"},
                    "extension_directory": {
                        "type": "string",
                        "description": "Path to HLA extension directory",
                    },
                    "name": {
                        "type": "string",
                        "description": "HLA class name",
                    },
                    "input_analyzer_index": {
                        "type": "integer",
                        "description": "Analyzer index of the input low-level analyzer",
                    },
                    "settings": {
                        "type": "object",
                        "description": "HLA-specific settings",
                    },
                    "label": {
                        "type": "string",
                        "description": "Display label",
                    },
                },
                "required": [
                    "capture_id",
                    "extension_directory",
                    "name",
                    "input_analyzer_index",
                ],
            },
        ),
        Tool(
            name="export_analyzer_data",
            description="Export decoded protocol data as CSV. Returns contents inline for small captures.",
            inputSchema={
                "type": "object",
                "properties": {
                    "capture_id": {"type": "string", "description": "Capture ID"},
                    "analyzer_index": {
                        "type": "integer",
                        "description": "Analyzer index from add_analyzer",
                    },
                    "output_path": {
                        "type": "string",
                        "description": "Output CSV file path (auto-generated if omitted)",
                    },
                    "radix": {
                        "type": "string",
                        "enum": ["hex", "dec", "bin", "ascii"],
                        "description": "Number format (default: hex)",
                    },
                    "max_rows": {
                        "type": "integer",
                        "description": "Max rows to return inline (default: 1000)",
                    },
                },
                "required": ["capture_id", "analyzer_index"],
            },
        ),
        Tool(
            name="export_raw_data",
            description="Export raw channel data as CSV files",
            inputSchema={
                "type": "object",
                "properties": {
                    "capture_id": {"type": "string", "description": "Capture ID"},
                    "digital_channels": {
                        "type": "array",
                        "items": {"type": "integer"},
                        "description": "Digital channels to export",
                    },
                    "analog_channels": {
                        "type": "array",
                        "items": {"type": "integer"},
                        "description": "Analog channels to export",
                    },
                    "output_dir": {
                        "type": "string",
                        "description": "Output directory (uses default if omitted)",
                    },
                },
                "required": ["capture_id"],
            },
        ),
        Tool(
            name="analyze_capture",
            description=(
                "Smart summary of decoded protocol data: packet counts, errors, "
                "addresses, timing, data preview. Works with I2C, SPI, UART, and others."
            ),
            inputSchema={
                "type": "object",
                "properties": {
                    "capture_id": {"type": "string", "description": "Capture ID"},
                    "analyzer_index": {
                        "type": "integer",
                        "description": "Analyzer index",
                    },
                },
                "required": ["capture_id", "analyzer_index"],
            },
        ),
        Tool(
            name="search_protocol_data",
            description="Search analyzer results for specific values or patterns",
            inputSchema={
                "type": "object",
                "properties": {
                    "capture_id": {"type": "string", "description": "Capture ID"},
                    "analyzer_index": {
                        "type": "integer",
                        "description": "Analyzer index",
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Search pattern (regex supported)",
                    },
                    "column": {
                        "type": "string",
                        "description": "Column to search (searches all if omitted)",
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Max results to return (default: 100)",
                    },
                },
                "required": ["capture_id", "analyzer_index", "pattern"],
            },
        ),
        Tool(
            name="get_timing_info",
            description=(
                "Calculate frequency, duty cycle, and pulse widths from raw digital channel data"
            ),
            inputSchema={
                "type": "object",
                "properties": {
                    "capture_id": {"type": "string", "description": "Capture ID"},
                    "channel": {
                        "type": "integer",
                        "description": "Digital channel number",
                    },
                },
                "required": ["capture_id", "channel"],
            },
        ),
        Tool(
            name="configure_trigger",
            description="Set up a digital trigger with optional linked channel conditions",
            inputSchema={
                "type": "object",
                "properties": {
                    "trigger_channel": {
                        "type": "integer",
                        "description": "Channel that triggers capture",
                    },
                    "trigger_type": {
                        "type": "string",
                        "enum": ["rising", "falling", "pulse_high", "pulse_low"],
                    },
                    "min_pulse_width_seconds": {
                        "type": "number",
                        "description": "Min pulse width for pulse triggers",
                    },
                    "max_pulse_width_seconds": {
                        "type": "number",
                        "description": "Max pulse width for pulse triggers",
                    },
                    "linked_channels": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "channel": {"type": "integer"},
                                "state": {
                                    "type": "string",
                                    "enum": ["high", "low"],
                                },
                            },
                            "required": ["channel", "state"],
                        },
                        "description": "Channels that must be in a specific state during trigger",
                    },
                    "after_trigger_seconds": {
                        "type": "number",
                        "description": "Seconds to capture after trigger",
                    },
                },
                "required": ["trigger_channel", "trigger_type"],
            },
        ),
        Tool(
            name="compare_captures",
            description=(
                "Compare decoded data from two captures for regression testing. "
                "Both captures must have the same analyzer type at the given index."
            ),
            inputSchema={
                "type": "object",
                "properties": {
                    "capture_id_a": {"type": "string", "description": "First capture ID"},
                    "capture_id_b": {"type": "string", "description": "Second capture ID"},
                    "analyzer_index_a": {
                        "type": "integer",
                        "description": "Analyzer index in first capture",
                    },
                    "analyzer_index_b": {
                        "type": "integer",
                        "description": "Analyzer index in second capture",
                    },
                },
                "required": [
                    "capture_id_a",
                    "capture_id_b",
                    "analyzer_index_a",
                    "analyzer_index_b",
                ],
            },
        ),
        Tool(
            name="stream_capture",
            description=(
                "Start a timed capture and return decoded data incrementally. "
                "Captures for the specified duration, adds the analyzer, and returns results."
            ),
            inputSchema={
                "type": "object",
                "properties": {
                    "device_id": {"type": "string", "description": "Device serial"},
                    "channels": {
                        "type": "array",
                        "items": {"type": "integer"},
                        "description": "Digital channels",
                    },
                    "sample_rate": {
                        "type": "integer",
                        "description": "Sample rate in Hz (default: 10000000)",
                    },
                    "duration_seconds": {
                        "type": "number",
                        "description": "Capture duration in seconds",
                    },
                    "analyzer_name": {
                        "type": "string",
                        "description": "Protocol analyzer to add",
                    },
                    "analyzer_settings": {
                        "type": "object",
                        "description": "Analyzer settings",
                    },
                    "logic_level": {
                        "type": "number",
                        "description": "Voltage threshold (Logic Pro only)",
                    },
                    "max_rows": {
                        "type": "integer",
                        "description": "Max rows to return (default: 1000)",
                    },
                },
                "required": [
                    "channels",
                    "duration_seconds",
                    "analyzer_name",
                    "analyzer_settings",
                ],
            },
        ),
        Tool(
            name="deep_analyze",
            description=(
                "Statistical analysis of captured data using numpy/pandas. "
                "For protocol data: timing distributions, throughput, error rates, address frequency. "
                "For raw signals: jitter, pulse width stats, FFT frequency spectrum."
            ),
            inputSchema={
                "type": "object",
                "properties": {
                    "capture_id": {"type": "string", "description": "Capture ID"},
                    "analyzer_index": {
                        "type": "integer",
                        "description": "Analyze decoded protocol data",
                    },
                    "channel": {
                        "type": "integer",
                        "description": "Analyze raw digital signal",
                    },
                    "analog_channel": {
                        "type": "integer",
                        "description": "Analyze raw analog signal",
                    },
                },
                "required": ["capture_id"],
            },
        ),
    ]

    @server.list_tools()
    async def list_tools() -> list[Tool]:
        return TOOLS

    @server.call_tool()
    async def call_tool(name: str, arguments: dict) -> list[TextContent]:
        try:
            return await _dispatch(name, arguments)
        except Exception as e:
            logger.exception("Tool %s failed", name)
            return _text(f"Error: {e}")

    async def _dispatch(name: str, args: dict) -> list[TextContent]:
        match name:
            case "get_app_info":
                return _handle_get_app_info()
            case "list_devices":
                return _handle_list_devices(args)
            case "start_capture":
                return _handle_start_capture(args)
            case "stop_capture":
                return _handle_stop_capture(args)
            case "wait_capture":
                return _handle_wait_capture(args)
            case "close_capture":
                return _handle_close_capture(args)
            case "save_capture":
                return _handle_save_capture(args)
            case "load_capture":
                return _handle_load_capture(args)
            case "add_analyzer":
                return _handle_add_analyzer(args)
            case "add_high_level_analyzer":
                return _handle_add_high_level_analyzer(args)
            case "export_analyzer_data":
                return _handle_export_analyzer_data(args)
            case "export_raw_data":
                return _handle_export_raw_data(args)
            case "analyze_capture":
                return _handle_analyze_capture(args)
            case "search_protocol_data":
                return _handle_search_protocol_data(args)
            case "get_timing_info":
                return _handle_get_timing_info(args)
            case "configure_trigger":
                return _handle_configure_trigger(args)
            case "compare_captures":
                return _handle_compare_captures(args)
            case "stream_capture":
                return _handle_stream_capture(args)
            case "deep_analyze":
                return _handle_deep_analyze(args)
            case _:
                return _text(f"Unknown tool: {name}")

    # ── Tool implementations ────────────────────────────────────────────

    def _handle_get_app_info() -> list[TextContent]:
        mgr = _get_manager()
        info = mgr.get_app_info()
        return _json_text({
            "app_version": info.app_version,
            "api_version": f"{info.api_version.major}.{info.api_version.minor}.{info.api_version.patch}",
            "app_pid": info.app_pid,
        })

    def _handle_list_devices(args: dict) -> list[TextContent]:
        mgr = _get_manager()
        include_sim = args.get("include_simulation", False)
        devices = mgr.get_devices(include_simulation_devices=include_sim)
        result = []
        for d in devices:
            entry = {
                "device_id": d.device_id,
                "device_type": d.device_type.name,
                "is_simulation": d.is_simulation,
            }
            rate_info = DEVICE_RATE_INFO.get(d.device_type.name)
            if rate_info:
                entry["rate_info"] = {
                    "max_digital": rate_info["max_digital"],
                    "max_analog": rate_info["max_analog"],
                    "suggested_pairs": [
                        list(p) for p in rate_info["suggested_pairs"]
                    ],
                }
            result.append(entry)
        return _json_text(result)

    def _handle_start_capture(args: dict) -> list[TextContent]:
        auto = _get_automation()
        mgr = _get_manager()

        channels = args.get("channels", [])
        analog_channels = args.get("analog_channels", [])
        sample_rate = args.get("sample_rate")
        analog_sample_rate = args.get("analog_sample_rate")
        logic_level = args.get("logic_level")
        glitch_filter_ns = args.get("glitch_filter_ns", {})

        # Auto-select sample rates when not specified
        if analog_channels and sample_rate is None and analog_sample_rate is None:
            sample_rate, analog_sample_rate = _DEFAULT_DIGITAL_ANALOG_RATE
        elif sample_rate is None:
            sample_rate = 10_000_000

        glitch_filters = []
        for ch_str, ns in glitch_filter_ns.items():
            glitch_filters.append(
                auto.GlitchFilterEntry(
                    channel_index=int(ch_str),
                    pulse_width_seconds=ns / 1e9,
                )
            )

        device_config = auto.LogicDeviceConfiguration(
            enabled_digital_channels=channels,
            enabled_analog_channels=analog_channels,
            digital_sample_rate=sample_rate,
            analog_sample_rate=analog_sample_rate,
            digital_threshold_volts=logic_level,
            glitch_filters=glitch_filters,
        )

        # Determine capture mode
        duration = args.get("duration_seconds")
        trigger_channel = args.get("trigger_channel")
        trigger_type_str = args.get("trigger_type")

        if trigger_channel is not None and trigger_type_str is not None:
            trigger_map = {
                "rising": auto.DigitalTriggerType.RISING,
                "falling": auto.DigitalTriggerType.FALLING,
                "pulse_high": auto.DigitalTriggerType.PULSE_HIGH,
                "pulse_low": auto.DigitalTriggerType.PULSE_LOW,
            }
            capture_mode = auto.DigitalTriggerCaptureMode(
                trigger_type=trigger_map[trigger_type_str],
                trigger_channel_index=trigger_channel,
                after_trigger_seconds=args.get("after_trigger_seconds"),
            )
            mode_name = "triggered"
        elif duration is not None:
            capture_mode = auto.TimedCaptureMode(duration_seconds=duration)
            mode_name = "timed"
        else:
            capture_mode = auto.ManualCaptureMode()
            mode_name = "manual"

        capture_config = auto.CaptureConfiguration(capture_mode=capture_mode)

        kwargs = {
            "device_configuration": device_config,
            "capture_configuration": capture_config,
        }
        if args.get("device_id"):
            kwargs["device_id"] = args["device_id"]

        try:
            capture = mgr.start_capture(**kwargs)
        except Exception as e:
            err_msg = str(e)
            if "sample rate" in err_msg.lower() or "rate" in err_msg.lower():
                hint = (
                    f"Sample rate error: {err_msg}\n\n"
                    "Hint: Digital+analog rates must be valid pairs. "
                    "Common valid pairs (digital, analog):\n"
                    "  125 MS/s + 12.5 MS/s\n"
                    "  50 MS/s + 12.5 MS/s\n"
                    "  50 MS/s + 6.25 MS/s\n"
                    "  25 MS/s + 3.125 MS/s\n"
                    "Use list_devices() to see suggested pairs for your device."
                )
                return _text(f"Error: {hint}")
            raise
        capture_id = _store_capture(capture)

        return _json_text({
            "capture_id": capture_id,
            "mode": mode_name,
            "digital_channels": channels,
            "analog_channels": analog_channels,
            "sample_rate": sample_rate,
            "analog_sample_rate": analog_sample_rate,
        })

    def _handle_stop_capture(args: dict) -> list[TextContent]:
        capture = _get_capture(args["capture_id"])
        capture.stop()
        return _text(f"Capture {args['capture_id']} stopped.")

    def _handle_wait_capture(args: dict) -> list[TextContent]:
        capture = _get_capture(args["capture_id"])
        capture.wait()
        return _text(f"Capture {args['capture_id']} completed.")

    def _handle_close_capture(args: dict) -> list[TextContent]:
        capture_id = args["capture_id"]
        capture = _get_capture(capture_id)
        capture.close()
        del state["captures"][capture_id]
        del state["analyzers"][capture_id]
        return _text(f"Capture {capture_id} closed.")

    def _handle_save_capture(args: dict) -> list[TextContent]:
        capture = _get_capture(args["capture_id"])
        filepath = _abs_path(args["filepath"])
        os.makedirs(os.path.dirname(filepath), exist_ok=True)
        capture.save_capture(filepath=filepath)
        return _text(f"Capture saved to {filepath}")

    def _handle_load_capture(args: dict) -> list[TextContent]:
        mgr = _get_manager()
        filepath = _abs_path(args["filepath"])
        capture = mgr.load_capture(filepath=filepath)
        capture_id = _store_capture(capture)
        return _json_text({"capture_id": capture_id, "loaded_from": filepath})

    def _handle_add_analyzer(args: dict) -> list[TextContent]:
        capture = _get_capture(args["capture_id"])
        kwargs = {
            "name": args["analyzer_name"],
            "settings": args.get("settings"),
        }
        if args.get("label"):
            kwargs["label"] = args["label"]

        handle = capture.add_analyzer(**kwargs)
        idx = _store_analyzer(args["capture_id"], handle)
        return _json_text({
            "analyzer_index": idx,
            "analyzer_name": args["analyzer_name"],
            "capture_id": args["capture_id"],
        })

    def _handle_add_high_level_analyzer(args: dict) -> list[TextContent]:
        capture = _get_capture(args["capture_id"])
        input_handle = _get_analyzer(
            args["capture_id"], args["input_analyzer_index"]
        )
        kwargs = {
            "extension_directory": args["extension_directory"],
            "name": args["name"],
            "input_analyzer": input_handle,
        }
        if args.get("settings"):
            kwargs["settings"] = args["settings"]
        if args.get("label"):
            kwargs["label"] = args["label"]

        handle = capture.add_high_level_analyzer(**kwargs)
        idx = _store_analyzer(args["capture_id"], handle)
        return _json_text({
            "analyzer_index": idx,
            "name": args["name"],
            "capture_id": args["capture_id"],
        })

    def _handle_export_analyzer_data(args: dict) -> list[TextContent]:
        auto = _get_automation()
        capture = _get_capture(args["capture_id"])
        handle = _get_analyzer(args["capture_id"], args["analyzer_index"])

        radix_map = {
            "hex": auto.RadixType.HEXADECIMAL,
            "dec": auto.RadixType.DECIMAL,
            "bin": auto.RadixType.BINARY,
            "ascii": auto.RadixType.ASCII,
        }
        radix = radix_map.get(args.get("radix", "hex"), auto.RadixType.HEXADECIMAL)

        output_path = args.get("output_path")
        if output_path:
            output_path = _abs_path(output_path)
        else:
            _ensure_output_dir()
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            output_path = os.path.join(
                config.output_dir,
                f"analyzer_{args['capture_id']}_{args['analyzer_index']}_{timestamp}.csv",
            )

        os.makedirs(os.path.dirname(output_path), exist_ok=True)

        export_config = auto.DataTableExportConfiguration(
            analyzer=handle, radix=radix
        )
        capture.export_data_table(filepath=output_path, analyzers=[export_config])

        # Read back and return inline if small enough
        max_rows = args.get("max_rows", 1000)
        return _read_csv_result(output_path, max_rows)

    def _handle_export_raw_data(args: dict) -> list[TextContent]:
        capture = _get_capture(args["capture_id"])
        output_dir = os.path.abspath(args.get("output_dir", config.output_dir))
        _ensure_output_dir()

        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        export_dir = os.path.join(output_dir, f"raw_{args['capture_id']}_{timestamp}")
        os.makedirs(export_dir, exist_ok=True)

        kwargs = {"directory": export_dir}
        if args.get("digital_channels"):
            kwargs["digital_channels"] = args["digital_channels"]
        if args.get("analog_channels"):
            kwargs["analog_channels"] = args["analog_channels"]

        capture.export_raw_data_csv(**kwargs)

        # List exported files
        files = os.listdir(export_dir) if os.path.isdir(export_dir) else []
        return _json_text({
            "output_dir": export_dir,
            "files": files,
        })

    def _handle_analyze_capture(args: dict) -> list[TextContent]:
        auto = _get_automation()
        capture = _get_capture(args["capture_id"])
        handle = _get_analyzer(args["capture_id"], args["analyzer_index"])

        # Export to a temp CSV file and analyze
        _ensure_output_dir()
        tmp_path = os.path.join(
            config.output_dir,
            f"_analyze_{args['capture_id']}_{args['analyzer_index']}.csv",
        )
        export_config = auto.DataTableExportConfiguration(
            analyzer=handle, radix=auto.RadixType.HEXADECIMAL
        )
        capture.export_data_table(filepath=tmp_path, analyzers=[export_config])

        csv_content = _read_file(tmp_path)
        # Detect protocol from column headers
        headers = _get_csv_headers(csv_content)
        headers_lower = [h.lower() for h in headers]

        if any("sda" in h or "address" in h for h in headers_lower):
            result = analyze_i2c_data(csv_content)
        elif any("mosi" in h or "miso" in h for h in headers_lower):
            result = analyze_spi_data(csv_content)
        elif any("data" in h and len(headers) <= 4 for h in headers_lower):
            result = analyze_uart_data(csv_content)
        else:
            # Generic analysis
            rows = list(csv.DictReader(io.StringIO(csv_content)))
            result = {
                "total_rows": len(rows),
                "columns": headers,
                "first_rows": rows[:5],
                "last_rows": rows[-5:] if len(rows) > 5 else [],
            }

        # Clean up temp file
        try:
            os.remove(tmp_path)
        except OSError:
            pass

        return _json_text(result)

    def _handle_search_protocol_data(args: dict) -> list[TextContent]:
        auto = _get_automation()
        capture = _get_capture(args["capture_id"])
        handle = _get_analyzer(args["capture_id"], args["analyzer_index"])

        _ensure_output_dir()
        tmp_path = os.path.join(
            config.output_dir,
            f"_search_{args['capture_id']}_{args['analyzer_index']}.csv",
        )
        export_config = auto.DataTableExportConfiguration(
            analyzer=handle, radix=auto.RadixType.HEXADECIMAL
        )
        capture.export_data_table(filepath=tmp_path, analyzers=[export_config])

        csv_content = _read_file(tmp_path)
        matches = search_csv_data(
            csv_content,
            pattern=args["pattern"],
            column=args.get("column"),
            max_results=args.get("max_results", 100),
        )

        try:
            os.remove(tmp_path)
        except OSError:
            pass

        return _json_text({"pattern": args["pattern"], "matches": matches})

    def _handle_get_timing_info(args: dict) -> list[TextContent]:
        capture = _get_capture(args["capture_id"])
        channel = args["channel"]

        _ensure_output_dir()
        tmp_dir = os.path.join(config.output_dir, f"_timing_{args['capture_id']}")
        os.makedirs(tmp_dir, exist_ok=True)

        capture.export_raw_data_csv(
            directory=tmp_dir, digital_channels=[channel]
        )

        # Find the exported CSV file
        csv_files = [f for f in os.listdir(tmp_dir) if f.endswith(".csv")]
        if not csv_files:
            return _text("No raw data exported for this channel.")

        csv_path = os.path.join(tmp_dir, csv_files[0])
        csv_content = _read_file(csv_path)

        # Clean up
        for f in csv_files:
            try:
                os.remove(os.path.join(tmp_dir, f))
            except OSError:
                pass
        try:
            os.rmdir(tmp_dir)
        except OSError:
            pass

        result = compute_timing_info(csv_content, channel)
        return _json_text(result)

    def _handle_configure_trigger(args: dict) -> list[TextContent]:
        auto = _get_automation()
        trigger_map = {
            "rising": auto.DigitalTriggerType.RISING,
            "falling": auto.DigitalTriggerType.FALLING,
            "pulse_high": auto.DigitalTriggerType.PULSE_HIGH,
            "pulse_low": auto.DigitalTriggerType.PULSE_LOW,
        }
        state_map = {
            "high": auto.DigitalTriggerLinkedChannelState.HIGH,
            "low": auto.DigitalTriggerLinkedChannelState.LOW,
        }

        linked = []
        for lc in args.get("linked_channels", []):
            linked.append(auto.DigitalTriggerLinkedChannel(
                channel_index=lc["channel"],
                state=state_map[lc["state"]],
            ))

        trigger_config = auto.DigitalTriggerCaptureMode(
            trigger_type=trigger_map[args["trigger_type"]],
            trigger_channel_index=args["trigger_channel"],
            min_pulse_width_seconds=args.get("min_pulse_width_seconds"),
            max_pulse_width_seconds=args.get("max_pulse_width_seconds"),
            linked_channels=linked,
            after_trigger_seconds=args.get("after_trigger_seconds"),
        )

        # Store the trigger config for the next start_capture call
        state["pending_trigger"] = trigger_config
        return _json_text({
            "status": "configured",
            "trigger_channel": args["trigger_channel"],
            "trigger_type": args["trigger_type"],
            "linked_channels": len(linked),
            "note": "Use start_capture with trigger_channel and trigger_type, or this config will be used automatically on next start_capture.",
        })

    def _handle_compare_captures(args: dict) -> list[TextContent]:
        auto = _get_automation()
        capture_a = _get_capture(args["capture_id_a"])
        capture_b = _get_capture(args["capture_id_b"])
        handle_a = _get_analyzer(args["capture_id_a"], args["analyzer_index_a"])
        handle_b = _get_analyzer(args["capture_id_b"], args["analyzer_index_b"])

        _ensure_output_dir()

        # Export both captures
        path_a = os.path.join(config.output_dir, f"_cmp_a_{args['capture_id_a']}.csv")
        path_b = os.path.join(config.output_dir, f"_cmp_b_{args['capture_id_b']}.csv")

        config_a = auto.DataTableExportConfiguration(
            analyzer=handle_a, radix=auto.RadixType.HEXADECIMAL
        )
        config_b = auto.DataTableExportConfiguration(
            analyzer=handle_b, radix=auto.RadixType.HEXADECIMAL
        )
        capture_a.export_data_table(filepath=path_a, analyzers=[config_a])
        capture_b.export_data_table(filepath=path_b, analyzers=[config_b])

        csv_a = _read_file(path_a)
        csv_b = _read_file(path_b)

        rows_a = list(csv.DictReader(io.StringIO(csv_a)))
        rows_b = list(csv.DictReader(io.StringIO(csv_b)))

        # Compare
        result = {
            "capture_a_rows": len(rows_a),
            "capture_b_rows": len(rows_b),
            "row_count_match": len(rows_a) == len(rows_b),
            "differences": [],
        }

        min_len = min(len(rows_a), len(rows_b))
        for i in range(min_len):
            diffs = {}
            for key in rows_a[i]:
                if key in rows_b[i] and rows_a[i][key] != rows_b[i][key]:
                    diffs[key] = {"a": rows_a[i][key], "b": rows_b[i][key]}
            if diffs:
                result["differences"].append({"row": i, "diffs": diffs})
                if len(result["differences"]) >= 50:
                    result["truncated"] = True
                    break

        result["total_differences"] = len(result["differences"])
        if len(rows_a) != len(rows_b):
            result["extra_rows_in"] = (
                "a" if len(rows_a) > len(rows_b) else "b"
            )
            result["extra_row_count"] = abs(len(rows_a) - len(rows_b))

        # Clean up
        for p in [path_a, path_b]:
            try:
                os.remove(p)
            except OSError:
                pass

        return _json_text(result)

    def _handle_stream_capture(args: dict) -> list[TextContent]:
        """One-shot: capture + analyze + return results."""
        auto = _get_automation()
        mgr = _get_manager()

        channels = args.get("channels", [])
        sample_rate = args.get("sample_rate", 10_000_000)
        duration = args["duration_seconds"]
        logic_level = args.get("logic_level")

        device_config = auto.LogicDeviceConfiguration(
            enabled_digital_channels=channels,
            digital_sample_rate=sample_rate,
            digital_threshold_volts=logic_level,
        )
        capture_config = auto.CaptureConfiguration(
            capture_mode=auto.TimedCaptureMode(duration_seconds=duration)
        )

        kwargs = {
            "device_configuration": device_config,
            "capture_configuration": capture_config,
        }
        if args.get("device_id"):
            kwargs["device_id"] = args["device_id"]

        capture = mgr.start_capture(**kwargs)
        capture_id = _store_capture(capture)
        capture.wait()

        # Add analyzer
        analyzer_kwargs = {
            "name": args["analyzer_name"],
            "settings": args.get("analyzer_settings"),
        }
        handle = capture.add_analyzer(**analyzer_kwargs)
        analyzer_idx = _store_analyzer(capture_id, handle)

        # Export
        _ensure_output_dir()
        tmp_path = os.path.join(
            config.output_dir,
            f"_stream_{capture_id}.csv",
        )
        export_config = auto.DataTableExportConfiguration(
            analyzer=handle, radix=auto.RadixType.HEXADECIMAL
        )
        capture.export_data_table(filepath=tmp_path, analyzers=[export_config])

        max_rows = args.get("max_rows", 1000)
        result = _read_csv_result(tmp_path, max_rows)

        try:
            os.remove(tmp_path)
        except OSError:
            pass

        # Prepend capture info
        import json
        info = json.dumps({
            "capture_id": capture_id,
            "analyzer_index": analyzer_idx,
            "duration_seconds": duration,
            "note": "Capture remains open. Use close_capture to release.",
        }, indent=2)
        return [TextContent(type="text", text=info)] + result

    def _handle_deep_analyze(args: dict) -> list[TextContent]:
        capture = _get_capture(args["capture_id"])
        capture_id = args["capture_id"]
        _ensure_output_dir()

        if "analyzer_index" in args:
            # Protocol deep analysis
            auto = _get_automation()
            handle = _get_analyzer(capture_id, args["analyzer_index"])
            tmp_path = os.path.join(
                config.output_dir,
                f"_deep_{capture_id}_{args['analyzer_index']}.csv",
            )
            export_config = auto.DataTableExportConfiguration(
                analyzer=handle, radix=auto.RadixType.HEXADECIMAL
            )
            capture.export_data_table(filepath=tmp_path, analyzers=[export_config])
            csv_content = _read_file(tmp_path)

            # Detect protocol from column headers
            headers = _get_csv_headers(csv_content)
            headers_lower = [h.lower() for h in headers]
            if any("sda" in h or "address" in h for h in headers_lower):
                protocol = "I2C"
            elif any("mosi" in h or "miso" in h for h in headers_lower):
                protocol = "SPI"
            elif any("data" in h and len(headers) <= 4 for h in headers_lower):
                protocol = "UART"
            else:
                protocol = "unknown"

            result = deep_analyze_protocol(csv_content, protocol)
            try:
                os.remove(tmp_path)
            except OSError:
                pass
            return _json_text(result)

        elif "channel" in args:
            # Raw digital deep analysis
            channel = args["channel"]
            tmp_dir = os.path.join(config.output_dir, f"_deep_dig_{capture_id}")
            os.makedirs(tmp_dir, exist_ok=True)
            capture.export_raw_data_csv(
                directory=tmp_dir, digital_channels=[channel]
            )
            csv_files = [f for f in os.listdir(tmp_dir) if f.endswith(".csv")]
            if not csv_files:
                return _text("No raw data exported for this channel.")
            csv_content = _read_file(os.path.join(tmp_dir, csv_files[0]))
            result = deep_analyze_digital(csv_content, channel)
            for f in csv_files:
                try:
                    os.remove(os.path.join(tmp_dir, f))
                except OSError:
                    pass
            try:
                os.rmdir(tmp_dir)
            except OSError:
                pass
            return _json_text(result)

        elif "analog_channel" in args:
            # Raw analog deep analysis
            channel = args["analog_channel"]
            tmp_dir = os.path.join(config.output_dir, f"_deep_ana_{capture_id}")
            os.makedirs(tmp_dir, exist_ok=True)
            capture.export_raw_data_csv(
                directory=tmp_dir, analog_channels=[channel]
            )
            csv_files = [f for f in os.listdir(tmp_dir) if f.endswith(".csv")]
            if not csv_files:
                return _text("No raw data exported for this channel.")
            csv_content = _read_file(os.path.join(tmp_dir, csv_files[0]))
            result = deep_analyze_analog(csv_content)
            for f in csv_files:
                try:
                    os.remove(os.path.join(tmp_dir, f))
                except OSError:
                    pass
            try:
                os.rmdir(tmp_dir)
            except OSError:
                pass
            return _json_text(result)

        else:
            return _text(
                "Error: Provide one of: analyzer_index (protocol), "
                "channel (digital), or analog_channel (analog)"
            )

    # ── Helpers ──────────────────────────────────────────────────────────

    def _read_file(path: str) -> str:
        with open(path, "r") as f:
            return f.read()

    def _get_csv_headers(csv_content: str) -> list[str]:
        reader = csv.reader(io.StringIO(csv_content))
        try:
            return next(reader)
        except StopIteration:
            return []

    def _read_csv_result(
        filepath: str, max_rows: int
    ) -> list[TextContent]:
        content = _read_file(filepath)
        reader = csv.reader(io.StringIO(content))
        rows = list(reader)

        if not rows:
            return _json_text({"row_count": 0, "file_path": filepath})

        headers = rows[0]
        data_rows = rows[1:]
        total = len(data_rows)

        if total <= max_rows:
            return _json_text({
                "row_count": total,
                "columns": headers,
                "file_path": filepath,
                "data": content,
            })
        else:
            # Truncate and return summary
            truncated = io.StringIO()
            writer = csv.writer(truncated)
            writer.writerow(headers)
            for row in data_rows[:max_rows]:
                writer.writerow(row)
            return _json_text({
                "row_count": total,
                "columns": headers,
                "file_path": filepath,
                "truncated": True,
                "rows_returned": max_rows,
                "data": truncated.getvalue(),
            })

    return server
