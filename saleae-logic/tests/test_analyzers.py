"""Tests for analyzer and export tools.

Requires Logic 2 running with automation server enabled.
Run with: pytest tests/test_analyzers.py -v
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


@pytest.fixture
async def capture_with_data(server):
    """Create a timed capture for analyzer tests."""
    content = await _call_tool(server, "start_capture", {
        "channels": [0, 1],
        "sample_rate": 1_000_000,
        "duration_seconds": 0.5,
    })
    capture_id = json.loads(content[0].text)["capture_id"]
    await _call_tool(server, "wait_capture", {"capture_id": capture_id})
    yield capture_id
    await _call_tool(server, "close_capture", {"capture_id": capture_id})


@pytest.mark.asyncio
async def test_add_analyzer(server, capture_with_data):
    """Test adding an I2C analyzer."""
    content = await _call_tool(server, "add_analyzer", {
        "capture_id": capture_with_data,
        "analyzer_name": "I2C",
        "settings": {"SDA": 0, "SCL": 1},
    })
    data = json.loads(content[0].text)
    assert "analyzer_index" in data
    assert data["analyzer_name"] == "I2C"


@pytest.mark.asyncio
async def test_export_analyzer_data(server, capture_with_data):
    """Test exporting analyzer data."""
    # Add analyzer
    content = await _call_tool(server, "add_analyzer", {
        "capture_id": capture_with_data,
        "analyzer_name": "I2C",
        "settings": {"SDA": 0, "SCL": 1},
    })
    idx = json.loads(content[0].text)["analyzer_index"]

    # Export
    content = await _call_tool(server, "export_analyzer_data", {
        "capture_id": capture_with_data,
        "analyzer_index": idx,
        "radix": "hex",
    })
    data = json.loads(content[0].text)
    assert "row_count" in data
    assert "columns" in data
    assert "file_path" in data


@pytest.mark.asyncio
async def test_export_raw_data(server, capture_with_data):
    """Test exporting raw channel data."""
    content = await _call_tool(server, "export_raw_data", {
        "capture_id": capture_with_data,
        "digital_channels": [0, 1],
    })
    data = json.loads(content[0].text)
    assert "output_dir" in data
    assert "files" in data
