"""Tests for capture lifecycle tools.

Requires Logic 2 running with automation server enabled and a device connected
(or simulation device).
Run with: pytest tests/test_capture.py -v
"""

import json

import pytest

from mcp.types import CallToolRequest

from saleae_logic.config import Config
from saleae_logic.server import create_server


@pytest.fixture
def config(tmp_path):
    return Config(output_dir=str(tmp_path / "captures"))


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
async def test_timed_capture_lifecycle(server, tmp_path):
    """Test start -> wait -> save -> close lifecycle with timed capture."""
    content = await _call_tool(server, "start_capture", {
        "channels": [0, 1],
        "sample_rate": 1_000_000,
        "duration_seconds": 0.5,
    })
    data = json.loads(content[0].text)
    assert "capture_id" in data
    assert data["mode"] == "timed"
    capture_id = data["capture_id"]

    # Wait for completion
    content = await _call_tool(server, "wait_capture", {"capture_id": capture_id})
    assert "completed" in content[0].text.lower()

    # Save
    sal_path = str(tmp_path / "test.sal")
    content = await _call_tool(server, "save_capture", {
        "capture_id": capture_id,
        "filepath": sal_path,
    })
    assert "saved" in content[0].text.lower()

    # Close
    content = await _call_tool(server, "close_capture", {"capture_id": capture_id})
    assert "closed" in content[0].text.lower()


@pytest.mark.asyncio
async def test_manual_capture_lifecycle(server):
    """Test manual start -> stop -> close."""
    content = await _call_tool(server, "start_capture", {
        "channels": [0],
        "sample_rate": 1_000_000,
    })
    data = json.loads(content[0].text)
    assert data["mode"] == "manual"
    capture_id = data["capture_id"]

    # Stop
    content = await _call_tool(server, "stop_capture", {"capture_id": capture_id})
    assert "stopped" in content[0].text.lower()

    # Close
    await _call_tool(server, "close_capture", {"capture_id": capture_id})


@pytest.mark.asyncio
async def test_load_capture(server, tmp_path):
    """Test save then load."""
    content = await _call_tool(server, "start_capture", {
        "channels": [0],
        "sample_rate": 1_000_000,
        "duration_seconds": 0.2,
    })
    capture_id = json.loads(content[0].text)["capture_id"]
    await _call_tool(server, "wait_capture", {"capture_id": capture_id})

    sal_path = str(tmp_path / "reload_test.sal")
    await _call_tool(server, "save_capture", {"capture_id": capture_id, "filepath": sal_path})
    await _call_tool(server, "close_capture", {"capture_id": capture_id})

    # Load it back
    content = await _call_tool(server, "load_capture", {"filepath": sal_path})
    data = json.loads(content[0].text)
    assert "capture_id" in data
    new_id = data["capture_id"]

    # Clean up
    await _call_tool(server, "close_capture", {"capture_id": new_id})
