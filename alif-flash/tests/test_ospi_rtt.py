"""Tests for OSPI RTT flash programmer: protocol, chunking, address handling, CRC."""

import json
import os
import struct
import tempfile
import zlib
from unittest.mock import MagicMock, patch

import pytest

from alif_flash.ospi_rtt import (
    CMD_ERASE,
    CMD_HEADER_FMT,
    CMD_HEADER_SIZE,
    CMD_PING,
    CMD_READ,
    CMD_READ_ID,
    CMD_RESET_FLASH,
    CMD_VERIFY,
    CMD_WRITE,
    DEVICE,
    MAX_WRITE_CHUNK,
    OSPI_XIP_BASE,
    RESP_FLAG,
    RESP_HEADER_FMT,
    RESP_HEADER_SIZE,
    SECTOR_SIZE,
    STATUS_BAD_PARAM,
    STATUS_OK,
    STATUS_TIMEOUT,
    OspiProgrammer,
    OspiProgrammerError,
)


class TestProtocolConstants:
    """Verify protocol constants match firmware protocol.h."""

    def test_cmd_header_size(self):
        assert CMD_HEADER_SIZE == 12
        assert struct.calcsize(CMD_HEADER_FMT) == CMD_HEADER_SIZE

    def test_resp_header_size(self):
        assert RESP_HEADER_SIZE == 8
        assert struct.calcsize(RESP_HEADER_FMT) == RESP_HEADER_SIZE

    def test_max_write_chunk(self):
        assert MAX_WRITE_CHUNK == 4096

    def test_sector_size(self):
        assert SECTOR_SIZE == 0x10000

    def test_xip_base(self):
        assert OSPI_XIP_BASE == 0xC0000000

    def test_resp_flag(self):
        assert RESP_FLAG == 0x80

    def test_device_name(self):
        assert DEVICE == "AE722F80F55D5_M55_HP"


class TestCommandPacking:
    """Test command header packing/unpacking."""

    def test_ping_header(self):
        header = struct.pack(CMD_HEADER_FMT, CMD_PING, 0, 1, 0, 0)
        assert len(header) == CMD_HEADER_SIZE
        cmd_id, flags, seq, addr, length = struct.unpack(CMD_HEADER_FMT, header)
        assert cmd_id == CMD_PING
        assert flags == 0
        assert seq == 1
        assert addr == 0
        assert length == 0

    def test_write_header(self):
        header = struct.pack(CMD_HEADER_FMT, CMD_WRITE, 0, 42,
                             0xC0000000, 256)
        cmd_id, flags, seq, addr, length = struct.unpack(CMD_HEADER_FMT, header)
        assert cmd_id == CMD_WRITE
        assert seq == 42
        assert addr == 0xC0000000
        assert length == 256

    def test_erase_header(self):
        header = struct.pack(CMD_HEADER_FMT, CMD_ERASE, 0, 100,
                             0x00000000, 0x10000)
        cmd_id, _, seq, addr, length = struct.unpack(CMD_HEADER_FMT, header)
        assert cmd_id == CMD_ERASE
        assert seq == 100
        assert addr == 0
        assert length == SECTOR_SIZE

    def test_verify_header(self):
        header = struct.pack(CMD_HEADER_FMT, CMD_VERIFY, 0, 7,
                             0x100000, 4096)
        cmd_id, _, seq, addr, length = struct.unpack(CMD_HEADER_FMT, header)
        assert cmd_id == CMD_VERIFY
        assert addr == 0x100000
        assert length == 4096


class TestResponseParsing:
    """Test response header parsing."""

    def test_ok_response(self):
        resp = struct.pack(RESP_HEADER_FMT, CMD_PING | RESP_FLAG,
                           STATUS_OK, 1, 0)
        resp_id, status, seq, length = struct.unpack(RESP_HEADER_FMT, resp)
        assert resp_id == CMD_PING | RESP_FLAG
        assert status == STATUS_OK
        assert seq == 1
        assert length == 0

    def test_error_response(self):
        resp = struct.pack(RESP_HEADER_FMT, CMD_ERASE | RESP_FLAG,
                           STATUS_TIMEOUT, 5, 0)
        _, status, seq, _ = struct.unpack(RESP_HEADER_FMT, resp)
        assert status == STATUS_TIMEOUT
        assert seq == 5

    def test_verify_response_with_crc(self):
        crc = 0xDEADBEEF
        resp = struct.pack(RESP_HEADER_FMT, CMD_VERIFY | RESP_FLAG,
                           STATUS_OK, 3, 4)
        resp += struct.pack("<I", crc)
        _, status, _, length = struct.unpack(RESP_HEADER_FMT, resp[:RESP_HEADER_SIZE])
        assert status == STATUS_OK
        assert length == 4
        actual_crc = struct.unpack("<I", resp[RESP_HEADER_SIZE:])[0]
        assert actual_crc == crc

    def test_read_id_response(self):
        resp = struct.pack(RESP_HEADER_FMT, CMD_READ_ID | RESP_FLAG,
                           STATUS_OK, 1, 1)
        resp += bytes([0x9D])
        _, status, _, length = struct.unpack(RESP_HEADER_FMT, resp[:RESP_HEADER_SIZE])
        assert status == STATUS_OK
        assert length == 1
        assert resp[RESP_HEADER_SIZE] == 0x9D


class MockJLink:
    """Mock pylink.JLink for testing OspiProgrammer without hardware."""

    def __init__(self):
        self._down_buffer = bytearray()  # Host -> target
        self._up_buffer = bytearray()    # Target -> host
        self._responses = []             # Queue of (resp_header, data) tuples

    def queue_response(self, cmd_id, status, seq, data=b""):
        """Queue a response that will be returned on next rtt_read."""
        header = struct.pack(RESP_HEADER_FMT, cmd_id | RESP_FLAG,
                             status, seq, len(data))
        self._up_buffer.extend(header + data)

    def rtt_write(self, channel, data):
        """Simulate RTT write (host -> target)."""
        if isinstance(data, list):
            data = bytes(data)
        self._down_buffer.extend(data)
        return len(data)

    def rtt_read(self, channel, num_bytes):
        """Simulate RTT read (target -> host)."""
        available = min(num_bytes, len(self._up_buffer))
        if available == 0:
            return []
        result = list(self._up_buffer[:available])
        self._up_buffer = self._up_buffer[available:]
        return result


class TestOspiProgrammer:
    """Test OspiProgrammer with mock JLink."""

    def test_ping(self):
        jlink = MockJLink()
        prog = OspiProgrammer(jlink)
        # Queue response with version string
        jlink.queue_response(CMD_PING, STATUS_OK, 1, b"OSPI-RTT v1.0")
        result = prog.ping()
        assert result == "OSPI-RTT v1.0"

    def test_read_id(self):
        jlink = MockJLink()
        prog = OspiProgrammer(jlink)
        jlink.queue_response(CMD_READ_ID, STATUS_OK, 1, bytes([0x9D]))
        result = prog.read_id()
        assert result == 0x9D

    def test_erase(self):
        jlink = MockJLink()
        prog = OspiProgrammer(jlink)
        jlink.queue_response(CMD_ERASE, STATUS_OK, 1)
        prog.erase(0xC0000000, 0x10000)  # Should not raise

    def test_erase_timeout(self):
        jlink = MockJLink()
        prog = OspiProgrammer(jlink)
        jlink.queue_response(CMD_ERASE, STATUS_TIMEOUT, 1)
        with pytest.raises(OspiProgrammerError, match="TIMEOUT"):
            prog.erase(0x0, 0x10000)

    def test_program_small(self):
        """Program data smaller than one chunk."""
        jlink = MockJLink()
        prog = OspiProgrammer(jlink)
        data = b"\xAA" * 256
        jlink.queue_response(CMD_WRITE, STATUS_OK, 1)
        prog.program(0x0, data)  # Should not raise

        # Verify command was sent with correct data
        assert len(jlink._down_buffer) == CMD_HEADER_SIZE + 256

    def test_program_chunking(self):
        """Verify data is split into MAX_WRITE_CHUNK-sized pieces."""
        jlink = MockJLink()
        prog = OspiProgrammer(jlink)
        data = b"\xBB" * (MAX_WRITE_CHUNK * 2 + 100)

        # Queue 3 responses (2 full chunks + 1 partial)
        jlink.queue_response(CMD_WRITE, STATUS_OK, 1)
        jlink.queue_response(CMD_WRITE, STATUS_OK, 2)
        jlink.queue_response(CMD_WRITE, STATUS_OK, 3)

        prog.program(0x0, data)

        # Should have 3 writes: 4096 + 4096 + 100 = 8292
        total_written = len(jlink._down_buffer)
        expected = 3 * CMD_HEADER_SIZE + len(data)
        assert total_written == expected

    def test_program_progress_callback(self):
        """Verify progress callback is called."""
        jlink = MockJLink()
        prog = OspiProgrammer(jlink)
        data = b"\xCC" * (MAX_WRITE_CHUNK + 100)
        jlink.queue_response(CMD_WRITE, STATUS_OK, 1)
        jlink.queue_response(CMD_WRITE, STATUS_OK, 2)

        progress_calls = []
        prog.program(0x0, data, progress_cb=lambda w, t: progress_calls.append((w, t)))

        assert len(progress_calls) == 2
        assert progress_calls[0] == (MAX_WRITE_CHUNK, len(data))
        assert progress_calls[1] == (len(data), len(data))

    def test_verify_crc(self):
        jlink = MockJLink()
        prog = OspiProgrammer(jlink)
        expected_crc = 0x12345678
        jlink.queue_response(CMD_VERIFY, STATUS_OK, 1,
                             struct.pack("<I", expected_crc))
        result = prog.verify_crc(0x0, 4096)
        assert result == expected_crc

    def test_read(self):
        jlink = MockJLink()
        prog = OspiProgrammer(jlink)
        test_data = b"\x01\x02\x03\x04"
        jlink.queue_response(CMD_READ, STATUS_OK, 1, test_data)
        result = prog.read(0x0, 4)
        assert result == test_data

    def test_reset_flash(self):
        jlink = MockJLink()
        prog = OspiProgrammer(jlink)
        jlink.queue_response(CMD_RESET_FLASH, STATUS_OK, 1)
        prog.reset_flash()  # Should not raise

    def test_sequence_numbers_increment(self):
        jlink = MockJLink()
        prog = OspiProgrammer(jlink)

        # Send 3 pings, each should have incrementing seq
        for i in range(1, 4):
            jlink.queue_response(CMD_PING, STATUS_OK, i, b"v1")
            prog.ping()

        # Parse the seq numbers from the commands sent
        seqs = []
        offset = 0
        for _ in range(3):
            _, _, seq, _, _ = struct.unpack(
                CMD_HEADER_FMT, jlink._down_buffer[offset:offset + CMD_HEADER_SIZE])
            seqs.append(seq)
            offset += CMD_HEADER_SIZE
        assert seqs == [1, 2, 3]

    def test_sequence_wraps(self):
        jlink = MockJLink()
        prog = OspiProgrammer(jlink)
        prog._seq = 0xFFFE

        jlink.queue_response(CMD_PING, STATUS_OK, 0xFFFF, b"v1")
        prog.ping()

        jlink.queue_response(CMD_PING, STATUS_OK, 0, b"v1")
        prog.ping()

        seqs = []
        offset = 0
        for _ in range(2):
            _, _, seq, _, _ = struct.unpack(
                CMD_HEADER_FMT, jlink._down_buffer[offset:offset + CMD_HEADER_SIZE])
            seqs.append(seq)
            offset += CMD_HEADER_SIZE
        assert seqs == [0xFFFF, 0]

    def test_bad_param_error(self):
        jlink = MockJLink()
        prog = OspiProgrammer(jlink)
        jlink.queue_response(CMD_WRITE, STATUS_BAD_PARAM, 1)
        with pytest.raises(OspiProgrammerError, match="BAD_PARAM"):
            prog.program(0x0, b"\x00" * 256)


class TestAddressHandling:
    """Test address conversion (XIP to flash-relative)."""

    def test_xip_address_in_erase(self):
        """XIP addresses should be passed through to firmware (firmware converts)."""
        jlink = MockJLink()
        prog = OspiProgrammer(jlink)
        jlink.queue_response(CMD_ERASE, STATUS_OK, 1)
        prog.erase(0xC0100000, 0x10000)

        _, _, _, addr, length = struct.unpack(
            CMD_HEADER_FMT, jlink._down_buffer[:CMD_HEADER_SIZE])
        assert addr == 0xC0100000
        assert length == 0x10000

    def test_flash_relative_address(self):
        """Flash-relative addresses pass through unchanged."""
        jlink = MockJLink()
        prog = OspiProgrammer(jlink)
        jlink.queue_response(CMD_ERASE, STATUS_OK, 1)
        prog.erase(0x100000, 0x10000)

        _, _, _, addr, _ = struct.unpack(
            CMD_HEADER_FMT, jlink._down_buffer[:CMD_HEADER_SIZE])
        assert addr == 0x100000


class TestCRC32:
    """Test CRC32 computation matches Python zlib.crc32."""

    def test_empty(self):
        assert zlib.crc32(b"") & 0xFFFFFFFF == 0x00000000

    def test_known_value(self):
        # "123456789" -> CRC32 = 0xCBF43926
        assert zlib.crc32(b"123456789") & 0xFFFFFFFF == 0xCBF43926

    def test_all_zeros(self):
        data = b"\x00" * 256
        crc = zlib.crc32(data) & 0xFFFFFFFF
        assert crc == 0x0D968558

    def test_all_ff(self):
        data = b"\xFF" * 256
        crc = zlib.crc32(data) & 0xFFFFFFFF
        assert crc == 0xFEA8A821


class TestFlashImages:
    """Test config-based multi-image programming."""

    def test_flash_images_skips_mram(self):
        """MRAM entries should be skipped."""
        jlink = MockJLink()
        prog = OspiProgrammer(jlink)

        config = {
            "TFA": {
                "disabled": False,
                "binary": "bl32.bin",
                "mramAddress": "0x80002000",
            },
            "ROOTFS": {
                "disabled": False,
                "binary": "rootfs.bin",
                "address": "0xC0000000",
            },
        }

        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = os.path.join(tmpdir, "config.json")
            with open(config_path, "w") as f:
                json.dump(config, f)

            # Create only the OSPI binary
            rootfs_data = b"\xAA" * 512
            with open(os.path.join(tmpdir, "rootfs.bin"), "wb") as f:
                f.write(rootfs_data)

            # Queue erase + write + verify responses
            expected_crc = zlib.crc32(rootfs_data) & 0xFFFFFFFF
            jlink.queue_response(CMD_ERASE, STATUS_OK, 1)
            jlink.queue_response(CMD_WRITE, STATUS_OK, 2)
            jlink.queue_response(CMD_VERIFY, STATUS_OK, 3,
                                 struct.pack("<I", expected_crc))

            results = prog.flash_images(config_path, verify=True)

        assert "ROOTFS" in results["images"]
        assert "TFA" not in results["images"]
        assert results["images"]["ROOTFS"]["status"] == "ok"
        assert results["images"]["ROOTFS"]["verified"] is True

    def test_flash_images_skips_disabled(self):
        """Disabled entries should be skipped."""
        config = {
            "KERNEL": {
                "disabled": True,
                "binary": "kernel.bin",
                "address": "0xC0800000",
            },
        }

        jlink = MockJLink()
        prog = OspiProgrammer(jlink)

        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = os.path.join(tmpdir, "config.json")
            with open(config_path, "w") as f:
                json.dump(config, f)

            results = prog.flash_images(config_path)

        assert len(results["images"]) == 0

    def test_flash_images_file_not_found(self):
        """Missing binary should report file_not_found."""
        config = {
            "ROOTFS": {
                "disabled": False,
                "binary": "nonexistent.bin",
                "address": "0xC0000000",
            },
        }

        jlink = MockJLink()
        prog = OspiProgrammer(jlink)

        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = os.path.join(tmpdir, "config.json")
            with open(config_path, "w") as f:
                json.dump(config, f)

            results = prog.flash_images(config_path)

        assert results["images"]["ROOTFS"]["status"] == "file_not_found"
