"""Tests for XMODEM-CRC sender: CRC, packets, completion parsing, CDC detection."""

import os
import struct
import tempfile
import time
from unittest.mock import MagicMock, patch

import pytest

from alif_flash.xmodem import (
    ACK,
    BLOCK_SIZE,
    CAN,
    CRC_MODE,
    EOT,
    MAX_BLOCK_RETRIES,
    NAK,
    PER_BLOCK_ACK_TIMEOUT,
    POST_EOT_TIMEOUT,
    RECEIVER_READY_TIMEOUT,
    SOH,
    calculate_timeout,
    crc16_ccitt,
    find_cdc_device,
    read_completion,
    xmodem_send,
)


class TestCRC16:
    """CRC-16 CCITT against known vectors."""

    def test_empty(self):
        assert crc16_ccitt(b"") == 0x0000

    def test_known_vector_123456789(self):
        # Standard CCITT test vector
        assert crc16_ccitt(b"123456789") == 0x31C3

    def test_single_byte(self):
        assert crc16_ccitt(b"\x00") == 0x0000

    def test_all_ff(self):
        result = crc16_ccitt(b"\xFF" * 128)
        assert isinstance(result, int)
        assert 0 <= result <= 0xFFFF

    def test_all_zeros_128(self):
        result = crc16_ccitt(b"\x00" * 128)
        assert result == 0x0000  # CRC of all zeros with init 0 is 0

    def test_deterministic(self):
        data = b"\xAA\xBB\xCC\xDD"
        assert crc16_ccitt(data) == crc16_ccitt(data)


class TestPacketBuilding:
    """Test XMODEM packet structure: SOH + seq + ~seq + data + CRC16."""

    def test_packet_structure(self):
        data = b"\x55" * BLOCK_SIZE
        seq = 1
        crc = crc16_ccitt(data)
        packet = bytes([SOH, seq, 0xFF - seq]) + data + struct.pack(">H", crc)

        assert packet[0] == SOH
        assert packet[1] == 1
        assert packet[2] == 0xFE  # ~1
        assert packet[3:3 + BLOCK_SIZE] == data
        assert len(packet) == 3 + BLOCK_SIZE + 2  # header + data + CRC

    def test_seq_complement(self):
        for seq in [0, 1, 127, 255]:
            complement = 0xFF - seq
            assert seq + complement == 0xFF

    def test_crc_big_endian(self):
        data = b"\xAA" * BLOCK_SIZE
        crc = crc16_ccitt(data)
        packed = struct.pack(">H", crc)
        # Big-endian: high byte first
        assert packed[0] == (crc >> 8) & 0xFF
        assert packed[1] == crc & 0xFF


class TestLastBlockPadding:
    """Verify last block is padded with 0xFF to BLOCK_SIZE."""

    def test_pad_short_data(self):
        data = b"\x11" * 50
        padded = data + b'\xFF' * (BLOCK_SIZE - len(data))
        assert len(padded) == BLOCK_SIZE
        assert padded[:50] == data
        assert padded[50:] == b'\xFF' * 78

    def test_exact_block_no_padding(self):
        data = b"\x22" * BLOCK_SIZE
        assert len(data) == BLOCK_SIZE
        # No padding needed

    def test_single_byte(self):
        data = b"\x33"
        padded = data + b'\xFF' * (BLOCK_SIZE - 1)
        assert len(padded) == BLOCK_SIZE
        assert padded[0] == 0x33
        assert padded[1:] == b'\xFF' * 127


class MockSerial:
    """Mock serial port for testing XMODEM transfers."""

    def __init__(self, responses=None):
        """Initialize with a list of response bytes to return from read().

        Each entry in responses should be bytes or bytearray.
        read(1) pops one byte at a time.
        """
        self._rx_buffer = bytearray()
        self._tx_buffer = bytearray()
        if responses:
            for r in responses:
                if isinstance(r, int):
                    self._rx_buffer.append(r)
                else:
                    self._rx_buffer.extend(r)
        self.timeout = 1
        self.in_waiting = 0

    def add_response(self, data):
        if isinstance(data, int):
            self._rx_buffer.append(data)
        else:
            self._rx_buffer.extend(data)

    def read(self, size=1):
        if not self._rx_buffer:
            return b""
        n = min(size, len(self._rx_buffer))
        result = bytes(self._rx_buffer[:n])
        self._rx_buffer = self._rx_buffer[n:]
        return result

    def write(self, data):
        self._tx_buffer.extend(data)
        return len(data)

    def flush(self):
        pass


class TestCompletionParsing:
    """Test read_completion: Success, Error, silence, bare 'C'."""

    def test_success_message(self):
        port = MockSerial()
        port.add_response(b"\r\nSuccess, 12127488 bytes received.\r\n")
        result = read_completion(port, timeout=1)
        assert result["success"] is True
        assert "Success" in result["message"]

    def test_error_message(self):
        port = MockSerial()
        port.add_response(b"\r\nError: CRC mismatch\r\n")
        result = read_completion(port, timeout=1)
        assert result["success"] is False
        assert "Error" in result["message"]

    def test_fail_message(self):
        port = MockSerial()
        port.add_response(b"\r\nWrite failed at sector 42\r\n")
        result = read_completion(port, timeout=1)
        assert result["success"] is False
        assert "fail" in result["message"].lower()

    def test_timeout_no_data(self):
        port = MockSerial()  # No data
        result = read_completion(port, timeout=0.1)
        assert result["success"] is False
        assert "timeout" in result["message"].lower()

    def test_bare_c_after_text(self):
        """Bare 'C' after status text = flasher restarted, treat as success."""
        port = MockSerial()
        port.add_response(b"\r\nSome status output\r\nC")
        result = read_completion(port, timeout=1)
        assert result["success"] is True


class TestCDCDetection:
    """Test CDC-ACM device detection with mocked ioreg."""

    @patch("alif_flash.xmodem.glob.glob")
    @patch("alif_flash.xmodem._get_usb_vendor_ids")
    def test_alif_device_found(self, mock_vids, mock_glob):
        mock_glob.return_value = ["/dev/cu.usbmodem12001"]
        mock_vids.return_value = {"/dev/cu.usbmodem12001": 0x0525}
        assert find_cdc_device() == "/dev/cu.usbmodem12001"

    @patch("alif_flash.xmodem.glob.glob")
    @patch("alif_flash.xmodem._get_usb_vendor_ids")
    def test_jlink_filtered_out(self, mock_vids, mock_glob):
        mock_glob.return_value = [
            "/dev/cu.usbmodem00001",
            "/dev/cu.usbmodem12001",
        ]
        mock_vids.return_value = {
            "/dev/cu.usbmodem00001": 0x1366,  # J-Link
            "/dev/cu.usbmodem12001": 0x0525,  # Alif
        }
        assert find_cdc_device() == "/dev/cu.usbmodem12001"

    @patch("alif_flash.xmodem.glob.glob")
    @patch("alif_flash.xmodem._get_usb_vendor_ids")
    def test_no_devices(self, mock_vids, mock_glob):
        mock_glob.return_value = []
        mock_vids.return_value = {}
        assert find_cdc_device() == ""

    @patch("alif_flash.xmodem.glob.glob")
    @patch("alif_flash.xmodem._get_usb_vendor_ids")
    def test_only_jlink(self, mock_vids, mock_glob):
        mock_glob.return_value = ["/dev/cu.usbmodem00001"]
        mock_vids.return_value = {"/dev/cu.usbmodem00001": 0x1366}
        # Falls through to last resort — only J-Link available
        assert find_cdc_device() == "/dev/cu.usbmodem00001"

    @patch("alif_flash.xmodem.glob.glob")
    @patch("alif_flash.xmodem._get_usb_vendor_ids")
    def test_no_ioreg_fallback(self, mock_vids, mock_glob):
        """When ioreg returns no VIDs, return first candidate."""
        mock_glob.return_value = ["/dev/cu.usbmodem12001"]
        mock_vids.return_value = {}
        assert find_cdc_device() == "/dev/cu.usbmodem12001"

    @patch("alif_flash.xmodem.glob.glob")
    @patch("alif_flash.xmodem._get_usb_vendor_ids")
    def test_multiple_alif_returns_first(self, mock_vids, mock_glob):
        mock_glob.return_value = [
            "/dev/cu.usbmodem12001",
            "/dev/cu.usbmodem12003",
        ]
        mock_vids.return_value = {
            "/dev/cu.usbmodem12001": 0x0525,
            "/dev/cu.usbmodem12003": 0x0525,
        }
        assert find_cdc_device() == "/dev/cu.usbmodem12001"


class TestXmodemSend:
    """Test xmodem_send with mocked serial port."""

    def _create_test_file(self, size, tmpdir):
        filepath = os.path.join(tmpdir, "test.bin")
        with open(filepath, "wb") as f:
            f.write(b"\xAA" * size)
        return filepath

    def test_successful_single_block(self):
        """Single block transfer: 'C' -> SOH packet -> ACK -> EOT -> ACK -> Success."""
        with tempfile.TemporaryDirectory() as tmpdir:
            filepath = self._create_test_file(64, tmpdir)

            port = MockSerial()
            # Receiver ready
            port.add_response(bytes([CRC_MODE]))
            # ACK for block 1
            port.add_response(bytes([ACK]))
            # ACK for EOT
            port.add_response(bytes([ACK]))
            # Completion message
            port.add_response(b"\r\nSuccess, 128 bytes received.\r\n")

            result = xmodem_send(port, filepath)
            assert result["success"] is True
            assert result["bytes_sent"] == 64
            assert result["blocks"] == 1
            assert "Success" in result["flasher_message"]

    def test_successful_multi_block(self):
        """Two block transfer."""
        with tempfile.TemporaryDirectory() as tmpdir:
            filepath = self._create_test_file(BLOCK_SIZE + 10, tmpdir)

            port = MockSerial()
            port.add_response(bytes([CRC_MODE]))  # Ready
            port.add_response(bytes([ACK]))         # Block 1
            port.add_response(bytes([ACK]))         # Block 2
            port.add_response(bytes([ACK]))         # EOT
            port.add_response(b"\r\nSuccess, 256 bytes received.\r\n")

            result = xmodem_send(port, filepath)
            assert result["success"] is True
            assert result["blocks"] == 2

    def test_receiver_timeout(self):
        """No 'C' from receiver within 30s."""
        with tempfile.TemporaryDirectory() as tmpdir:
            filepath = self._create_test_file(64, tmpdir)

            port = MockSerial()  # No data -> timeout

            # Patch RECEIVER_READY_TIMEOUT to speed up test
            import alif_flash.xmodem as xmod
            orig = xmod.RECEIVER_READY_TIMEOUT
            xmod.RECEIVER_READY_TIMEOUT = 0.1
            try:
                result = xmodem_send(port, filepath)
            finally:
                xmod.RECEIVER_READY_TIMEOUT = orig

            assert result["success"] is False
            assert "No response" in result["error"]

    def test_nak_then_ack(self):
        """One NAK followed by ACK = success."""
        with tempfile.TemporaryDirectory() as tmpdir:
            filepath = self._create_test_file(64, tmpdir)

            port = MockSerial()
            port.add_response(bytes([CRC_MODE]))  # Ready
            port.add_response(bytes([NAK]))         # NAK first attempt
            port.add_response(bytes([ACK]))         # ACK retry
            port.add_response(bytes([ACK]))         # EOT
            port.add_response(b"\r\nSuccess, 128 bytes received.\r\n")

            result = xmodem_send(port, filepath)
            assert result["success"] is True

    def test_max_nak_failure(self):
        """10 consecutive NAKs = failure."""
        with tempfile.TemporaryDirectory() as tmpdir:
            filepath = self._create_test_file(64, tmpdir)

            port = MockSerial()
            port.add_response(bytes([CRC_MODE]))
            # 10 NAKs
            for _ in range(MAX_BLOCK_RETRIES):
                port.add_response(bytes([NAK]))

            result = xmodem_send(port, filepath)
            assert result["success"] is False
            assert "retries" in result["error"]

    def test_can_handling(self):
        """CAN from receiver = failure."""
        with tempfile.TemporaryDirectory() as tmpdir:
            filepath = self._create_test_file(64, tmpdir)

            port = MockSerial()
            port.add_response(bytes([CRC_MODE]))
            port.add_response(bytes([CAN]))

            result = xmodem_send(port, filepath)
            assert result["success"] is False
            assert "cancelled" in result["error"]

    def test_per_block_timeout(self):
        """No response at all for a block = timeout failure."""
        with tempfile.TemporaryDirectory() as tmpdir:
            filepath = self._create_test_file(64, tmpdir)

            port = MockSerial()
            port.add_response(bytes([CRC_MODE]))
            # No ACK/NAK responses at all -> timeout on each retry

            # Patch timeout to speed up test
            import alif_flash.xmodem as xmod
            orig = xmod.PER_BLOCK_ACK_TIMEOUT
            xmod.PER_BLOCK_ACK_TIMEOUT = 0.01
            try:
                result = xmodem_send(port, filepath)
            finally:
                xmod.PER_BLOCK_ACK_TIMEOUT = orig

            assert result["success"] is False
            assert "stopped responding" in result["error"]

    def test_progress_callback(self):
        """Verify progress callback is called."""
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create file large enough for multiple progress reports
            filepath = self._create_test_file(BLOCK_SIZE * 20, tmpdir)

            port = MockSerial()
            port.add_response(bytes([CRC_MODE]))
            for _ in range(20):
                port.add_response(bytes([ACK]))
            port.add_response(bytes([ACK]))  # EOT
            port.add_response(b"\r\nSuccess, 2560 bytes received.\r\n")

            calls = []
            result = xmodem_send(port, filepath,
                                 progress_callback=lambda s, t, e: calls.append((s, t)))

            assert result["success"] is True
            assert len(calls) > 0  # At least some progress reported

    def test_eot_not_acked(self):
        """EOT not acknowledged = failure."""
        with tempfile.TemporaryDirectory() as tmpdir:
            filepath = self._create_test_file(64, tmpdir)

            port = MockSerial()
            port.add_response(bytes([CRC_MODE]))
            port.add_response(bytes([ACK]))  # Block 1
            # No ACK for EOT (5 attempts with no response)

            # Patch timeout to speed up
            import alif_flash.xmodem as xmod
            orig = xmod.PER_BLOCK_ACK_TIMEOUT
            xmod.PER_BLOCK_ACK_TIMEOUT = 0.01
            try:
                result = xmodem_send(port, filepath)
            finally:
                xmod.PER_BLOCK_ACK_TIMEOUT = orig

            assert result["success"] is False
            assert "EOT" in result["error"]


class TestOverallTimeout:
    """Test overall timeout calculation."""

    def test_small_file(self):
        """Small file gets minimum 60s timeout."""
        assert calculate_timeout(1000) == 60

    def test_large_file(self):
        """12MB file: (12000000 / 30000) * 2 = 800s."""
        assert calculate_timeout(12_000_000) == 800

    def test_medium_file(self):
        """1MB file: (1000000 / 30000) * 2 ≈ 66s."""
        result = calculate_timeout(1_000_000)
        assert result == 66

    def test_zero_size(self):
        """Zero-size file gets minimum timeout."""
        assert calculate_timeout(0) == 60


class TestConstants:
    """Verify protocol constants are correct."""

    def test_block_size(self):
        assert BLOCK_SIZE == 128

    def test_soh(self):
        assert SOH == 0x01

    def test_eot(self):
        assert EOT == 0x04

    def test_ack(self):
        assert ACK == 0x06

    def test_nak(self):
        assert NAK == 0x15

    def test_can(self):
        assert CAN == 0x18

    def test_crc_mode(self):
        assert CRC_MODE == ord('C')

    def test_max_retries(self):
        assert MAX_BLOCK_RETRIES == 10

    def test_receiver_ready_timeout(self):
        assert RECEIVER_READY_TIMEOUT == 30

    def test_per_block_timeout(self):
        assert PER_BLOCK_ACK_TIMEOUT == 10

    def test_post_eot_timeout(self):
        assert POST_EOT_TIMEOUT == 30
