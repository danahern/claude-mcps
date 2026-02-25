"""OSPI flash programming via RTT using pylink-square.

Programs OSPI NOR flash (IS25WX256) on the Alif E7 at ~500 KB/s
via a custom M55_HP firmware that receives commands over SEGGER RTT.

Requires:
  - pylink-square package
  - JLink shared library (installed with JLink tools)
  - OSPI programmer firmware loaded on M55_HP (via ATOC config)
"""

import json
import logging
import os
import struct
import time
import zlib

logger = logging.getLogger(__name__)

# RTT protocol constants (must match firmware protocol.h)
CMD_PING = 0x01
CMD_READ_ID = 0x02
CMD_ERASE = 0x03
CMD_WRITE = 0x04
CMD_VERIFY = 0x05
CMD_READ = 0x06
CMD_RESET_FLASH = 0x08

RESP_FLAG = 0x80

STATUS_OK = 0
STATUS_TIMEOUT = 1
STATUS_VERIFY_FAIL = 2
STATUS_BAD_PARAM = 3
STATUS_FLASH_ERR = 4

CMD_HEADER_FMT = "<BBHII"  # cmd_id(B), flags(B), seq(H), addr(I), length(I)
CMD_HEADER_SIZE = 12
RESP_HEADER_FMT = "<BBHI"  # resp_id(B), status(B), seq(H), length(I)
RESP_HEADER_SIZE = 8

MAX_WRITE_CHUNK = 4096
SECTOR_SIZE = 0x10000  # 64KB
OSPI_XIP_BASE = 0xC0000000

DEVICE = "AE722F80F55D5_M55_HP"
RTT_TIMEOUT = 5.0  # seconds


class OspiProgrammerError(Exception):
    """Error during OSPI RTT programming."""


class OspiProgrammer:
    """OSPI flash programmer using RTT over JLink.

    Uses pylink-square to communicate with the M55_HP firmware
    that handles OSPI flash operations.
    """

    def __init__(self, jlink):
        """Initialize with a connected pylink.JLink instance.

        The JLink must already be connected to the M55_HP core and
        RTT must be started.
        """
        self._jlink = jlink
        self._seq = 0

    def _next_seq(self):
        self._seq = (self._seq + 1) & 0xFFFF
        return self._seq

    def _send_cmd(self, cmd_id, addr=0, length=0, data=None):
        """Send a command to the firmware and wait for response."""
        seq = self._next_seq()

        # Pack header
        header = struct.pack(CMD_HEADER_FMT, cmd_id, 0, seq, addr, length)

        # Write command
        if data:
            payload = header + data
        else:
            payload = header

        written = 0
        while written < len(payload):
            chunk = payload[written:]
            n = self._jlink.rtt_write(0, list(chunk))
            if n > 0:
                written += n
            else:
                time.sleep(0.001)

        # Read response
        return self._read_response(cmd_id, seq)

    def _read_response(self, expected_cmd_id, expected_seq):
        """Read and parse a response from the firmware."""
        resp_data = b""
        deadline = time.monotonic() + RTT_TIMEOUT

        # Read header
        while len(resp_data) < RESP_HEADER_SIZE:
            if time.monotonic() > deadline:
                raise OspiProgrammerError(
                    f"Timeout waiting for response to cmd 0x{expected_cmd_id:02x}")
            chunk = self._jlink.rtt_read(0, RESP_HEADER_SIZE - len(resp_data))
            if chunk:
                resp_data += bytes(chunk)
            else:
                time.sleep(0.001)

        resp_id, status, seq, length = struct.unpack(
            RESP_HEADER_FMT, resp_data[:RESP_HEADER_SIZE])

        # Validate header
        if resp_id != (expected_cmd_id | RESP_FLAG):
            raise OspiProgrammerError(
                f"Unexpected response ID 0x{resp_id:02x}, "
                f"expected 0x{expected_cmd_id | RESP_FLAG:02x}")
        if seq != expected_seq:
            raise OspiProgrammerError(
                f"Sequence mismatch: got {seq}, expected {expected_seq}")

        # Read payload data if any
        payload = b""
        if length > 0:
            deadline = time.monotonic() + RTT_TIMEOUT
            while len(payload) < length:
                if time.monotonic() > deadline:
                    raise OspiProgrammerError(
                        f"Timeout reading {length} bytes of response data")
                chunk = self._jlink.rtt_read(0, length - len(payload))
                if chunk:
                    payload += bytes(chunk)
                else:
                    time.sleep(0.001)

        if status != STATUS_OK:
            status_names = {
                STATUS_TIMEOUT: "TIMEOUT",
                STATUS_VERIFY_FAIL: "VERIFY_FAIL",
                STATUS_BAD_PARAM: "BAD_PARAM",
                STATUS_FLASH_ERR: "FLASH_ERR",
            }
            raise OspiProgrammerError(
                f"Command 0x{expected_cmd_id:02x} failed: "
                f"{status_names.get(status, f'UNKNOWN({status})')}")

        return payload

    def ping(self):
        """Health check. Returns firmware version string."""
        payload = self._send_cmd(CMD_PING)
        return payload.decode("ascii", errors="replace")

    def read_id(self):
        """Read flash JEDEC ID. Returns manufacturer ID byte."""
        payload = self._send_cmd(CMD_READ_ID)
        return payload[0] if payload else 0

    def erase(self, addr, length):
        """Erase sectors covering the given range.

        Args:
            addr: Flash address (0-based or 0xC0xxxxxx).
            length: Number of bytes to erase (rounded up to sector boundary).
        """
        self._send_cmd(CMD_ERASE, addr=addr, length=length)

    def program(self, addr, data, progress_cb=None):
        """Program data to flash.

        Sends data in chunks of up to MAX_WRITE_CHUNK bytes.

        Args:
            addr: Flash address (0-based or 0xC0xxxxxx).
            data: Bytes to program.
            progress_cb: Optional callback(bytes_written, total_bytes).
        """
        total = len(data)
        offset = 0

        while offset < total:
            chunk_size = min(MAX_WRITE_CHUNK, total - offset)
            chunk = data[offset:offset + chunk_size]

            self._send_cmd(CMD_WRITE, addr=addr + offset,
                           length=chunk_size, data=chunk)

            offset += chunk_size
            if progress_cb:
                progress_cb(offset, total)

    def verify_crc(self, addr, length):
        """Compute CRC32 of flash region. Returns the CRC32 value."""
        payload = self._send_cmd(CMD_VERIFY, addr=addr, length=length)
        if len(payload) < 4:
            raise OspiProgrammerError("Verify response too short")
        return struct.unpack("<I", payload[:4])[0]

    def read(self, addr, length):
        """Read raw flash data. Returns bytes."""
        payload = self._send_cmd(CMD_READ, addr=addr, length=length)
        return payload

    def reset_flash(self):
        """Software reset the flash chip."""
        self._send_cmd(CMD_RESET_FLASH)

    def flash_image(self, addr, data, verify=True, progress_cb=None):
        """High-level: erase + program + optional verify for one image.

        Returns dict with results.
        """
        total = len(data)
        flash_addr = addr

        # Erase
        logger.info("Erasing %d bytes at 0x%x", total, flash_addr)
        self.erase(flash_addr, total)

        # Program
        logger.info("Programming %d bytes at 0x%x", total, flash_addr)
        self.program(flash_addr, data, progress_cb=progress_cb)

        result = {
            "address": f"0x{flash_addr:08x}",
            "size": total,
            "status": "ok",
        }

        # Verify
        if verify:
            logger.info("Verifying %d bytes at 0x%x", total, flash_addr)
            expected_crc = zlib.crc32(data) & 0xFFFFFFFF
            actual_crc = self.verify_crc(flash_addr, total)
            if actual_crc != expected_crc:
                result["status"] = "verify_failed"
                result["expected_crc"] = f"0x{expected_crc:08x}"
                result["actual_crc"] = f"0x{actual_crc:08x}"
                raise OspiProgrammerError(
                    f"Verify failed at 0x{flash_addr:08x}: "
                    f"expected CRC 0x{expected_crc:08x}, "
                    f"got 0x{actual_crc:08x}")
            result["crc32"] = f"0x{actual_crc:08x}"
            result["verified"] = True

        return result

    def flash_images(self, config_path, verify=True):
        """Flash all enabled images from an ATOC-style JSON config.

        Processes entries with 'address' or 'ospiAddress' fields
        (OSPI images only — MRAM images are skipped).

        Returns dict with per-image results.
        """
        with open(config_path) as f:
            config = json.load(f)

        config_dir = os.path.dirname(os.path.abspath(config_path))
        results = {}
        total_bytes = 0
        start_time = time.monotonic()

        for name, entry in config.items():
            if not isinstance(entry, dict):
                continue
            if entry.get("disabled", False):
                continue

            # Find address — only OSPI addresses
            addr_str = entry.get("address") or entry.get("ospiAddress")
            if not addr_str:
                continue

            addr = int(addr_str, 16) if isinstance(addr_str, str) else addr_str
            if addr < OSPI_XIP_BASE:
                continue

            binary = entry.get("binary")
            if not binary:
                continue

            # Resolve binary path
            bin_path = binary if os.path.isabs(binary) else \
                os.path.join(config_dir, binary)
            if not os.path.exists(bin_path):
                results[name] = {"status": "file_not_found", "binary": binary}
                continue

            with open(bin_path, "rb") as f:
                data = f.read()

            logger.info("Flashing %s: %s (%d bytes) -> 0x%x",
                        name, binary, len(data), addr)

            def progress(written, total, _name=name):
                pct = written * 100 // total
                logger.info("%s: %d/%d bytes (%d%%)", _name, written, total, pct)

            result = self.flash_image(addr, data, verify=verify,
                                      progress_cb=progress)
            result["binary"] = binary
            results[name] = result
            total_bytes += len(data)

        elapsed = time.monotonic() - start_time
        speed = total_bytes / elapsed if elapsed > 0 else 0

        return {
            "images": results,
            "total_bytes": total_bytes,
            "elapsed_s": round(elapsed, 1),
            "speed_kbs": round(speed / 1024, 1),
        }


def connect_and_program(config_path, verify=True):
    """Convenience: connect JLink, start RTT, program, close.

    Returns results dict.
    """
    import pylink

    jlink = pylink.JLink()
    try:
        jlink.open()
        jlink.connect(DEVICE, verbose=True)
        jlink.rtt_start()

        # Wait for RTT control block
        deadline = time.monotonic() + 5.0
        while time.monotonic() < deadline:
            try:
                jlink.rtt_read(0, 1)
                break
            except Exception:
                time.sleep(0.1)

        programmer = OspiProgrammer(jlink)

        # Verify firmware is alive
        version = programmer.ping()
        logger.info("OSPI programmer firmware: %s", version)

        # Verify flash ID
        flash_id = programmer.read_id()
        logger.info("Flash ID: 0x%02x", flash_id)
        if flash_id != 0x9D:
            raise OspiProgrammerError(
                f"Unexpected flash ID 0x{flash_id:02x}, expected 0x9D (ISSI)")

        results = programmer.flash_images(config_path, verify=verify)
        results["firmware_version"] = version
        results["flash_id"] = f"0x{flash_id:02x}"
        return results

    finally:
        try:
            jlink.rtt_stop()
        except Exception:
            pass
        jlink.close()
