"""Smoke tests for server startup — no hardware or Logic 2 required.

These tests verify the MCP server can be created, registers tools,
and wires up stdio transport correctly. They would have caught:
- run_stdio_async() bug (Server doesn't have that method)
- Relative path bug (Logic 2 resolves paths from its own CWD)
"""

import os
import sys

import pytest
from mcp.types import ListToolsRequest, CallToolRequest

from saleae_logic.config import Config, parse_args
from saleae_logic.server import create_server


# ── Server creation ─────────────────────────────────────────────────


def test_create_server_returns_valid_server():
    """Server object must have the methods needed for stdio transport."""
    config = Config()
    server = create_server(config)
    assert hasattr(server, "run"), "Server must have run() for stdio transport"
    assert hasattr(server, "create_initialization_options"), (
        "Server must have create_initialization_options()"
    )


def test_create_initialization_options():
    """create_initialization_options() must return without error."""
    config = Config()
    server = create_server(config)
    opts = server.create_initialization_options()
    assert opts is not None


def test_server_registers_tool_handlers():
    """Server must register ListTools and CallTool request handlers."""
    config = Config()
    server = create_server(config)
    assert ListToolsRequest in server.request_handlers
    assert CallToolRequest in server.request_handlers


def test_stdio_server_import():
    """The stdio transport module must be importable."""
    from mcp.server.stdio import stdio_server
    assert callable(stdio_server)


# ── Config paths ────────────────────────────────────────────────────


def test_parse_args_output_dir_is_absolute(monkeypatch):
    """output_dir must always be absolute — Logic 2 is a separate process."""
    monkeypatch.setattr(sys, "argv", ["saleae_logic"])
    config = parse_args()
    assert os.path.isabs(config.output_dir), (
        f"output_dir must be absolute, got: {config.output_dir}"
    )


def test_parse_args_relative_output_dir_becomes_absolute(monkeypatch):
    """Even explicit relative paths must be converted to absolute."""
    monkeypatch.setattr(sys, "argv", ["saleae_logic", "--output-dir", "./my_captures"])
    config = parse_args()
    assert os.path.isabs(config.output_dir)
    assert config.output_dir.endswith("my_captures")


def test_parse_args_absolute_output_dir_unchanged(monkeypatch):
    """Absolute paths should pass through unchanged."""
    monkeypatch.setattr(sys, "argv", ["saleae_logic", "--output-dir", "/tmp/saleae_out"])
    config = parse_args()
    assert config.output_dir == "/tmp/saleae_out"


# ── Tool registration ─────────────────────────────────────────────


@pytest.mark.asyncio
async def test_tool_count():
    """Server must register exactly 18 tools."""
    config = Config()
    server = create_server(config)
    req = ListToolsRequest(method="tools/list")
    result = await server.request_handlers[ListToolsRequest](req)
    tools = result.root.tools
    assert len(tools) == 18, f"expected 18 tools, got {len(tools)}"


@pytest.mark.asyncio
async def test_all_tool_names():
    """All 18 expected tool names must be registered."""
    expected = {
        "get_app_info", "list_devices", "start_capture", "stop_capture",
        "wait_capture", "close_capture", "save_capture", "load_capture",
        "add_analyzer", "add_high_level_analyzer", "export_analyzer_data",
        "export_raw_data", "analyze_capture", "search_protocol_data",
        "get_timing_info", "configure_trigger", "compare_captures",
        "stream_capture",
    }
    config = Config()
    server = create_server(config)
    req = ListToolsRequest(method="tools/list")
    result = await server.request_handlers[ListToolsRequest](req)
    actual = {t.name for t in result.root.tools}
    assert actual == expected


@pytest.mark.asyncio
async def test_unknown_tool_returns_error():
    """Calling an unknown tool should return an error message, not crash."""
    config = Config()
    server = create_server(config)
    req = CallToolRequest(
        method="tools/call",
        params={"name": "nonexistent_tool", "arguments": {}},
    )
    result = await server.request_handlers[CallToolRequest](req)
    content = result.root.content
    assert len(content) == 1
    assert "unknown tool" in content[0].text.lower()
