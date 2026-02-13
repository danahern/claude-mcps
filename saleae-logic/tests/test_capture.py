"""Tests for capture lifecycle tools.

Requires Logic 2 running with automation server enabled and a device connected
(or simulation device).
Run with: pytest tests/test_capture.py -v
"""

import json

import pytest

from saleae_logic.config import Config
from saleae_logic.server import create_server


@pytest.fixture
def config(tmp_path):
    return Config(output_dir=str(tmp_path / "captures"))


@pytest.fixture
def server(config):
    return create_server(config)


@pytest.mark.asyncio
async def test_timed_capture_lifecycle(server, tmp_path):
    """Test start → wait → save → close lifecycle with timed capture."""
    # Start a short timed capture
    result = await server.call_tool("start_capture", {
        "channels": [0, 1],
        "sample_rate": 1_000_000,
        "duration_seconds": 0.5,
    })
    data = json.loads(result[0].text)
    assert "capture_id" in data
    assert data["mode"] == "timed"
    capture_id = data["capture_id"]

    # Wait for completion
    result = await server.call_tool("wait_capture", {"capture_id": capture_id})
    assert "completed" in result[0].text.lower()

    # Save
    sal_path = str(tmp_path / "test.sal")
    result = await server.call_tool("save_capture", {
        "capture_id": capture_id,
        "filepath": sal_path,
    })
    assert "saved" in result[0].text.lower()

    # Close
    result = await server.call_tool("close_capture", {"capture_id": capture_id})
    assert "closed" in result[0].text.lower()


@pytest.mark.asyncio
async def test_manual_capture_lifecycle(server):
    """Test manual start → stop → close."""
    result = await server.call_tool("start_capture", {
        "channels": [0],
        "sample_rate": 1_000_000,
    })
    data = json.loads(result[0].text)
    assert data["mode"] == "manual"
    capture_id = data["capture_id"]

    # Stop
    result = await server.call_tool("stop_capture", {"capture_id": capture_id})
    assert "stopped" in result[0].text.lower()

    # Close
    await server.call_tool("close_capture", {"capture_id": capture_id})


@pytest.mark.asyncio
async def test_load_capture(server, tmp_path):
    """Test save then load."""
    # Create and save a capture
    result = await server.call_tool("start_capture", {
        "channels": [0],
        "sample_rate": 1_000_000,
        "duration_seconds": 0.2,
    })
    capture_id = json.loads(result[0].text)["capture_id"]
    await server.call_tool("wait_capture", {"capture_id": capture_id})

    sal_path = str(tmp_path / "reload_test.sal")
    await server.call_tool("save_capture", {"capture_id": capture_id, "filepath": sal_path})
    await server.call_tool("close_capture", {"capture_id": capture_id})

    # Load it back
    result = await server.call_tool("load_capture", {"filepath": sal_path})
    data = json.loads(result[0].text)
    assert "capture_id" in data
    new_id = data["capture_id"]

    # Clean up
    await server.call_tool("close_capture", {"capture_id": new_id})
