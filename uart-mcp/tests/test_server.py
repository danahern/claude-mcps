"""Server registration and dispatch tests."""

import pytest

from uart_mcp.server import create_server, TOOLS, _get_session


class TestToolRegistration:
    def test_tool_count(self):
        assert len(TOOLS) == 6

    def test_tool_names(self):
        names = {t.name for t in TOOLS}
        assert names == {
            "list_ports",
            "open_port",
            "close_port",
            "send_command",
            "read_output",
            "write_raw",
        }

    def test_required_fields(self):
        """Each tool with required fields should list them."""
        for tool in TOOLS:
            schema = tool.inputSchema
            required = schema.get("required", [])
            props = schema.get("properties", {})
            for req in required:
                assert req in props, f"Tool {tool.name}: required field '{req}' not in properties"

    def test_open_port_requires_port(self):
        tool = next(t for t in TOOLS if t.name == "open_port")
        assert "port" in tool.inputSchema["required"]

    def test_send_command_requires_session_and_command(self):
        tool = next(t for t in TOOLS if t.name == "send_command")
        assert "session_id" in tool.inputSchema["required"]
        assert "command" in tool.inputSchema["required"]


class TestGetSession:
    def test_valid_session(self):
        sessions = {"abc12345": "mock_session"}
        result = _get_session(sessions, "abc12345")
        assert result == "mock_session"

    def test_invalid_session(self):
        sessions = {}
        with pytest.raises(ValueError, match="No session with id"):
            _get_session(sessions, "nonexistent")

    def test_error_shows_active_sessions(self):
        sessions = {"abc12345": "s1", "def67890": "s2"}
        with pytest.raises(ValueError, match="abc12345"):
            _get_session(sessions, "nonexistent")


class TestCreateServer:
    def test_creates_server(self):
        server = create_server()
        assert server.name == "uart-mcp"


class TestDispatch:
    @pytest.mark.asyncio
    async def test_list_ports(self):
        from unittest.mock import patch
        from uart_mcp.server import _dispatch

        with patch("uart_mcp.serial_session.serial.tools.list_ports.comports", return_value=[]):
            result = await _dispatch("list_ports", {}, {})
            assert len(result) == 1
            assert '"count": 0' in result[0].text

    @pytest.mark.asyncio
    async def test_unknown_tool(self):
        from uart_mcp.server import _dispatch

        result = await _dispatch("bogus_tool", {}, {})
        assert "Unknown tool" in result[0].text

    @pytest.mark.asyncio
    async def test_close_invalid_session(self):
        from uart_mcp.server import _dispatch

        with pytest.raises(ValueError, match="No session"):
            await _dispatch("close_port", {"session_id": "bad"}, {})

    @pytest.mark.asyncio
    async def test_write_raw_hex(self):
        """Test hex decoding path in write_raw dispatch."""
        from unittest.mock import MagicMock, patch
        from uart_mcp.server import _dispatch

        mock_session = MagicMock()
        mock_session.write_raw.return_value = 2
        mock_session.session_id = "test1234"
        sessions = {"test1234": mock_session}

        result = await _dispatch(
            "write_raw",
            {"session_id": "test1234", "data": "0d0a", "hex": True},
            sessions,
        )
        mock_session.write_raw.assert_called_once_with(b"\r\n")
        assert '"bytes_written": 2' in result[0].text
