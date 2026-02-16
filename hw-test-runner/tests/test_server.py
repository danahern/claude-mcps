"""Tests for MCP server tool definitions."""

from hw_test_runner.server import TOOLS, create_server


class TestToolDefinitions:
    def test_tool_count(self):
        assert len(TOOLS) == 9

    def test_tool_names(self):
        names = {t.name for t in TOOLS}
        expected = {
            "ble_discover",
            "ble_read",
            "ble_write",
            "ble_subscribe",
            "wifi_provision",
            "wifi_scan_aps",
            "wifi_status",
            "wifi_factory_reset",
            "tcp_throughput",
        }
        assert names == expected

    def test_required_fields(self):
        """Verify required fields are set for tools that need them."""
        by_name = {t.name: t for t in TOOLS}

        assert by_name["ble_read"].inputSchema["required"] == [
            "address",
            "characteristic_uuid",
        ]
        assert by_name["ble_write"].inputSchema["required"] == [
            "address",
            "characteristic_uuid",
            "data",
        ]
        assert by_name["wifi_provision"].inputSchema["required"] == ["ssid", "psk"]
        assert by_name["tcp_throughput"].inputSchema["required"] == ["host", "mode"]

    def test_optional_tools_have_no_required(self):
        """Tools with all-optional params should not have required."""
        by_name = {t.name: t for t in TOOLS}
        for name in ["ble_discover", "wifi_scan_aps", "wifi_status", "wifi_factory_reset"]:
            schema = by_name[name].inputSchema
            assert "required" not in schema or schema.get("required") == [], (
                f"{name} should have no required fields"
            )

    def test_tcp_throughput_enum(self):
        by_name = {t.name: t for t in TOOLS}
        mode_prop = by_name["tcp_throughput"].inputSchema["properties"]["mode"]
        assert mode_prop["enum"] == ["upload", "download", "echo"]

    def test_create_server(self):
        server = create_server()
        assert server is not None
        assert server.name == "hw-test-runner"
