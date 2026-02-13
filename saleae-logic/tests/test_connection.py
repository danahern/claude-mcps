"""Tests for get_app_info and list_devices.

Requires Logic 2 running with automation server enabled.
Run with: pytest tests/test_connection.py -v
"""

import json

import pytest
import pytest_asyncio

from saleae_logic.config import Config
from saleae_logic.server import create_server


@pytest.fixture
def config():
    return Config()


@pytest.fixture
def server(config):
    return create_server(config)


@pytest.mark.asyncio
async def test_get_app_info(server):
    """Test that get_app_info returns version info."""
    tools = await server.list_tools()
    tool_names = [t.name for t in tools]
    assert "get_app_info" in tool_names

    # This will fail if Logic 2 is not running
    result = await server.call_tool("get_app_info", {})
    assert len(result) == 1
    data = json.loads(result[0].text)
    assert "app_version" in data
    assert "api_version" in data
    assert "app_pid" in data


@pytest.mark.asyncio
async def test_list_devices(server):
    """Test that list_devices returns device list."""
    result = await server.call_tool("list_devices", {"include_simulation": True})
    assert len(result) == 1
    data = json.loads(result[0].text)
    assert isinstance(data, list)
    # With simulation, should have at least one device
    if data:
        assert "device_id" in data[0]
        assert "device_type" in data[0]
