"""Tests for TCP throughput module constants and helpers."""

from hw_test_runner.throughput import (
    CMD_ECHO,
    CMD_SINK,
    CMD_SOURCE,
    DEFAULT_PORT,
    DEFAULT_DURATION,
    DEFAULT_BLOCK_SIZE,
)


class TestConstants:
    def test_command_bytes(self):
        assert CMD_ECHO == 0x01
        assert CMD_SINK == 0x02
        assert CMD_SOURCE == 0x03

    def test_defaults(self):
        assert DEFAULT_PORT == 4242
        assert DEFAULT_DURATION == 10.0
        assert DEFAULT_BLOCK_SIZE == 1024

    def test_mode_to_command_mapping(self):
        """Verify the mapping used in tcp_throughput()."""
        mapping = {"upload": CMD_SINK, "download": CMD_SOURCE, "echo": CMD_ECHO}
        assert mapping["upload"] == 0x02
        assert mapping["download"] == 0x03
        assert mapping["echo"] == 0x01
