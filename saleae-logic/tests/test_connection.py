"""Tests for get_app_info and list_devices.

Requires Logic 2 running with automation server enabled.
Run with: pytest tests/test_connection.py -v
"""

import json

import pytest

from mcp.types import CallToolRequest, ListToolsRequest

from saleae_logic.config import Config
from saleae_logic.server import create_server


@pytest.fixture
def config():
    return Config()


@pytest.fixture
def server(config):
    return create_server(config)


async def _call_tool(server, name, arguments=None):
    """Invoke a tool via the registered MCP handler."""
    req = CallToolRequest(
        method="tools/call",
        params={"name": name, "arguments": arguments or {}},
    )
    result = await server.request_handlers[CallToolRequest](req)
    return result.root.content


@pytest.mark.asyncio
async def test_get_app_info(server):
    """Test that get_app_info returns version info."""
    # Verify the tool is registered
    req = ListToolsRequest(method="tools/list")
    result = await server.request_handlers[ListToolsRequest](req)
    tool_names = [t.name for t in result.root.tools]
    assert "get_app_info" in tool_names

    # This will fail if Logic 2 is not running
    content = await _call_tool(server, "get_app_info")
    assert len(content) == 1
    data = json.loads(content[0].text)
    assert "app_version" in data
    assert "api_version" in data
    assert "app_pid" in data


@pytest.mark.asyncio
async def test_list_devices(server):
    """Test that list_devices returns device list."""
    content = await _call_tool(server, "list_devices", {"include_simulation": True})
    assert len(content) == 1
    data = json.loads(content[0].text)
    assert isinstance(data, list)
    # With simulation, should have at least one device
    if data:
        assert "device_id" in data[0]
        assert "device_type" in data[0]
