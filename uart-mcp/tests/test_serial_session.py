"""Tests for serial session core logic — mock serial port."""

import time
from unittest.mock import MagicMock, patch, PropertyMock

import pytest

from uart_mcp.serial_session import (
    SerialSession,
    generate_session_id,
    list_serial_ports,
    _strip_echo,
)


class TestGenerateSessionId:
    def test_length(self):
        sid = generate_session_id()
        assert len(sid) == 8

    def test_hex_chars(self):
        sid = generate_session_id()
        int(sid, 16)  # Should not raise

    def test_unique(self):
        ids = {generate_session_id() for _ in range(100)}
        assert len(ids) == 100


class TestStripEcho:
    def test_strips_echoed_command(self):
        text = "ls -la\nfile1.txt\nfile2.txt"
        result = _strip_echo(text, "ls -la")
        assert result == "file1.txt\nfile2.txt"

    def test_no_echo(self):
        text = "file1.txt\nfile2.txt"
        result = _strip_echo(text, "ls -la")
        assert result == "file1.txt\nfile2.txt"

    def test_empty_output(self):
        result = _strip_echo("", "ls")
        assert result == ""

    def test_command_with_extra_chars(self):
        # Serial echo may include CR or other chars around the command
        text = "ls -la\r\nfile1.txt"
        result = _strip_echo(text, "ls -la")
        assert "file1.txt" in result

    def test_only_echo(self):
        result = _strip_echo("ls -la", "ls -la")
        assert result == ""


class TestListSerialPorts:
    @patch("uart_mcp.serial_session.serial.tools.list_ports.comports")
    def test_returns_port_info(self, mock_comports):
        mock_port = MagicMock()
        mock_port.device = "/dev/cu.usbserial-123"
        mock_port.description = "USB Serial"
        mock_port.hwid = "USB VID:PID=0403:6001"
        mock_port.manufacturer = "FTDI"
        mock_comports.return_value = [mock_port]

        ports = list_serial_ports()
        assert len(ports) == 1
        assert ports[0]["port"] == "/dev/cu.usbserial-123"
        assert ports[0]["manufacturer"] == "FTDI"

    @patch("uart_mcp.serial_session.serial.tools.list_ports.comports")
    def test_empty(self, mock_comports):
        mock_comports.return_value = []
        ports = list_serial_ports()
        assert ports == []


class TestSerialSessionOpenClose:
    @patch("uart_mcp.serial_session.serial.Serial")
    def test_open_creates_serial(self, mock_serial_cls):
        mock_ser = MagicMock()
        mock_ser.is_open = True
        mock_serial_cls.return_value = mock_ser

        session = SerialSession(port="/dev/ttyUSB0", baud=9600)
        session.open()

        mock_serial_cls.assert_called_once_with(
            port="/dev/ttyUSB0",
            baudrate=9600,
            bytesize=8,
            parity="N",
            stopbits=1,
            timeout=0.1,
        )
        mock_ser.reset_input_buffer.assert_called_once()
        assert session.is_open

    @patch("uart_mcp.serial_session.serial.Serial")
    def test_open_twice_raises(self, mock_serial_cls):
        mock_ser = MagicMock()
        mock_ser.is_open = True
        mock_serial_cls.return_value = mock_ser

        session = SerialSession(port="/dev/ttyUSB0")
        session.open()
        with pytest.raises(RuntimeError, match="already open"):
            session.open()

    @patch("uart_mcp.serial_session.serial.Serial")
    def test_close(self, mock_serial_cls):
        mock_ser = MagicMock()
        mock_ser.is_open = True
        mock_serial_cls.return_value = mock_ser

        session = SerialSession(port="/dev/ttyUSB0")
        session.open()
        session.close()

        mock_ser.close.assert_called_once()
        assert not session.is_open

    def test_close_when_not_open(self):
        session = SerialSession(port="/dev/ttyUSB0")
        session.close()  # Should not raise


class TestSerialSessionWriteRaw:
    @patch("uart_mcp.serial_session.serial.Serial")
    def test_write_raw(self, mock_serial_cls):
        mock_ser = MagicMock()
        mock_ser.is_open = True
        mock_ser.write.return_value = 5
        mock_serial_cls.return_value = mock_ser

        session = SerialSession(port="/dev/ttyUSB0")
        session.open()
        n = session.write_raw(b"hello")
        assert n == 5
        mock_ser.write.assert_called_with(b"hello")

    def test_write_raw_not_open(self):
        session = SerialSession(port="/dev/ttyUSB0")
        with pytest.raises(RuntimeError, match="not open"):
            session.write_raw(b"hello")


class TestSerialSessionReadOutput:
    @patch("uart_mcp.serial_session.serial.Serial")
    def test_read_output_returns_data(self, mock_serial_cls):
        mock_ser = MagicMock()
        mock_ser.is_open = True
        # First call returns data, subsequent calls return empty (idle timeout)
        mock_ser.in_waiting = 0
        call_count = 0

        def read_side_effect(n):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                return b"hello world"
            return b""

        mock_ser.read.side_effect = read_side_effect
        mock_serial_cls.return_value = mock_ser

        session = SerialSession(port="/dev/ttyUSB0")
        session.open()
        output = session.read_output(timeout=0.1)
        assert "hello world" in output

    def test_read_output_not_open(self):
        session = SerialSession(port="/dev/ttyUSB0")
        with pytest.raises(RuntimeError, match="not open"):
            session.read_output()


class TestSerialSessionSendCommand:
    @patch("uart_mcp.serial_session.serial.Serial")
    def test_send_command_basic(self, mock_serial_cls):
        mock_ser = MagicMock()
        mock_ser.is_open = True
        mock_ser.in_waiting = 0
        call_count = 0

        def read_side_effect(n):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                return b"echo ok\r\nok\r\n"
            return b""

        mock_ser.read.side_effect = read_side_effect
        mock_serial_cls.return_value = mock_ser

        session = SerialSession(port="/dev/ttyUSB0")
        session.open()
        output = session.send_command("echo ok", timeout=0.1)
        mock_ser.write.assert_called_with(b"echo ok\r\n")
        # Echo should be filtered
        assert "ok" in output
        assert output.startswith("echo ok") is False

    @patch("uart_mcp.serial_session.serial.Serial")
    def test_send_command_no_echo_filter(self, mock_serial_cls):
        mock_ser = MagicMock()
        mock_ser.is_open = True
        mock_ser.in_waiting = 0
        call_count = 0

        def read_side_effect(n):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                return b"echo ok\r\nok\r\n"
            return b""

        mock_ser.read.side_effect = read_side_effect
        mock_serial_cls.return_value = mock_ser

        session = SerialSession(port="/dev/ttyUSB0", echo_filter=False)
        session.open()
        output = session.send_command("echo ok", timeout=0.1)
        assert "echo ok" in output

    @patch("uart_mcp.serial_session.serial.Serial")
    def test_send_command_wait_for_prompt(self, mock_serial_cls):
        mock_ser = MagicMock()
        mock_ser.is_open = True
        mock_ser.in_waiting = 0
        call_count = 0

        def read_side_effect(n):
            nonlocal call_count
            call_count += 1
            if call_count == 1:
                return b"ls\r\nfile.txt\r\n# "
            return b""

        mock_ser.read.side_effect = read_side_effect
        mock_serial_cls.return_value = mock_ser

        session = SerialSession(port="/dev/ttyUSB0")
        session.open()
        output = session.send_command("ls", timeout=0.1, wait_for=r"# $")
        assert "file.txt" in output

    def test_send_command_not_open(self):
        session = SerialSession(port="/dev/ttyUSB0")
        with pytest.raises(RuntimeError, match="not open"):
            session.send_command("echo ok")
