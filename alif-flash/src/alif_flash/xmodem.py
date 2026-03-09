"""XMODEM-CRC sender for Alif USB-to-OSPI flasher.

Sends a binary image to the flasher firmware over USB CDC-ACM using
XMODEM-CRC protocol (128-byte blocks with CRC-16).

Auto-detects Alif CDC-ACM devices (VID 0x0525) on macOS, filtering
out J-Link VCOM (VID 0x1366).
"""

import glob
import logging
import os
import re
import struct
import subprocess
import time

import serial

logger = logging.getLogger(__name__)

# XMODEM constants
SOH = 0x01   # 128-byte block header
EOT = 0x04   # End of transmission
ACK = 0x06   # Acknowledge
NAK = 0x15   # Negative acknowledge
CAN = 0x18   # Cancel
CRC_MODE = ord('C')  # CRC mode request

BLOCK_SIZE = 128  # Standard XMODEM (firmware doesn't support 1K)

# Timeout defaults (seconds)
RECEIVER_READY_TIMEOUT = 30
PER_BLOCK_ACK_TIMEOUT = 10
POST_EOT_TIMEOUT = 30
MAX_BLOCK_RETRIES = 10


def crc16_ccitt(data: bytes) -> int:
    """CRC-16 CCITT (poly 0x1021, init 0x0000)."""
    crc = 0
    for byte in data:
        crc ^= byte << 8
        for _ in range(8):
            if crc & 0x8000:
                crc = (crc << 1) ^ 0x1021
            else:
                crc <<= 1
            crc &= 0xFFFF
    return crc


def _get_usb_vendor_ids() -> dict[str, int]:
    """Parse ioreg on macOS to map USB modem device names to vendor IDs.

    Returns dict mapping device basename (e.g., 'usbmodem12001') to VID.
    """
    vendor_map = {}
    try:
        output = subprocess.check_output(
            ["ioreg", "-r", "-c", "IOUSBHostDevice", "-l"],
            text=True, timeout=5,
        )
    except (subprocess.SubprocessError, FileNotFoundError):
        return vendor_map

    # Parse ioreg output: look for idVendor and IODialinDevice pairs
    # within the same device block
    current_vid = None
    for line in output.splitlines():
        vid_match = re.search(r'"idVendor"\s*=\s*(\d+)', line)
        if vid_match:
            current_vid = int(vid_match.group(1))
        dialin_match = re.search(r'"IODialinDevice"\s*=\s*"(/dev/cu\.\w+)"', line)
        if dialin_match and current_vid is not None:
            device_path = dialin_match.group(1)
            vendor_map[device_path] = current_vid

    return vendor_map


def find_cdc_device() -> str:
    """Find the Alif USB CDC-ACM device on macOS.

    Filters by VID 0x0525 (Linux USB gadget / Alif CDC-ACM).
    Excludes J-Link VCOM (VID 0x1366).

    Returns device path or empty string if not found.
    """
    candidates = sorted(glob.glob("/dev/cu.usbmodem*"))
    if not candidates:
        return ""

    vendor_map = _get_usb_vendor_ids()

    # Filter to VID 0x0525 only
    alif_devices = [d for d in candidates if vendor_map.get(d) == 0x0525]
    if alif_devices:
        return alif_devices[0]

    # If ioreg didn't find VIDs (non-macOS or parse failure), fall back
    # to returning first candidate that isn't obviously a J-Link
    jlink_devices = [d for d in candidates if vendor_map.get(d) == 0x1366]
    non_jlink = [d for d in candidates if d not in jlink_devices]
    if non_jlink:
        return non_jlink[0]

    # Last resort: return first candidate
    return candidates[0] if candidates else ""


def read_completion(port: serial.Serial, timeout: float = POST_EOT_TIMEOUT) -> dict:
    """Read post-transfer output from flasher, looking for completion.

    After EOT/ACK, the flasher prints status text then restarts XMODEM
    (sending 'C' characters). We look for:
    - "Success" line -> success
    - "Error" or "fail" (case-insensitive) -> failure
    - Bare 'C' after status text -> treat as success (flasher moved on)
    - Timeout with no data -> failure

    Returns dict with 'success', 'message', and 'raw_output' keys.
    """
    buffer = ""
    has_text = False  # True once we've seen non-'C' text
    deadline = time.monotonic() + timeout

    while time.monotonic() < deadline:
        data = port.read(port.in_waiting or 1)
        if not data:
            continue

        text = data.decode("ascii", errors="replace")
        buffer += text

        # Reset deadline on activity
        deadline = time.monotonic() + timeout

        for ch in text:
            if ch == 'C' and has_text:
                # Bare 'C' after text = flasher restarted XMODEM loop
                return {
                    "success": True,
                    "message": buffer.strip(),
                    "raw_output": buffer,
                }
            elif ch not in ('C', '\r', '\n', ' '):
                has_text = True

        # Check for completion keywords in accumulated buffer
        for line in buffer.splitlines():
            line_stripped = line.strip()
            if "Success" in line_stripped:
                return {
                    "success": True,
                    "message": line_stripped,
                    "raw_output": buffer,
                }
            if re.search(r"error|fail", line_stripped, re.IGNORECASE):
                return {
                    "success": False,
                    "message": line_stripped,
                    "raw_output": buffer,
                }

    # Timeout
    return {
        "success": False,
        "message": "No confirmation from flasher (timeout)",
        "raw_output": buffer,
    }


def xmodem_send(port: serial.Serial, filepath: str,
                progress_callback=None) -> dict:
    """Send a file via XMODEM-CRC with 128-byte blocks.

    Args:
        port: Open serial port.
        filepath: Path to binary file to send.
        progress_callback: Optional callable(bytes_sent, total_bytes, elapsed).

    Returns dict with:
        success, bytes_sent, elapsed_seconds, speed_kbps, blocks,
        flasher_message, error (if failed).
    """
    file_size = os.path.getsize(filepath)
    total_blocks = (file_size + BLOCK_SIZE - 1) // BLOCK_SIZE

    # Calculate overall timeout: (file_size / 30000) * 2, minimum 60s
    overall_timeout = max(60, (file_size / 30000) * 2)

    logger.info("XMODEM send: %s (%d bytes, %d blocks, timeout %.0fs)",
                filepath, file_size, total_blocks, overall_timeout)

    overall_deadline = time.monotonic() + overall_timeout

    # --- Phase 1: Wait for receiver ready ('C') ---
    logger.info("Waiting for receiver ready signal...")
    port.timeout = 1  # read timeout for polling
    deadline = time.monotonic() + RECEIVER_READY_TIMEOUT
    pre_text = ""
    mode = None

    while time.monotonic() < deadline:
        if time.monotonic() > overall_deadline:
            return {"success": False, "bytes_sent": 0, "elapsed_seconds": 0,
                    "speed_kbps": 0, "blocks": 0,
                    "error": f"Overall timeout ({overall_timeout:.0f}s)"}
        b = port.read(1)
        if not b:
            continue
        if b[0] == CRC_MODE:
            mode = "crc"
            break
        elif b[0] == NAK:
            mode = "checksum"
            break
        else:
            pre_text += b.decode("ascii", errors="replace")

    if mode is None:
        msg = "No response from receiver (timeout 30s)"
        if pre_text:
            msg += f" — pre-transfer output: {pre_text.strip()}"
        return {"success": False, "bytes_sent": 0, "elapsed_seconds": 0,
                "speed_kbps": 0, "blocks": 0, "error": msg}

    logger.info("Receiver ready (mode: %s)", mode)

    # --- Phase 2: Send blocks ---
    start_time = time.monotonic()
    last_progress_pct = -1

    with open(filepath, "rb") as f:
        block_num = 1

        while True:
            if time.monotonic() > overall_deadline:
                elapsed = time.monotonic() - start_time
                return {"success": False,
                        "bytes_sent": (block_num - 1) * BLOCK_SIZE,
                        "elapsed_seconds": round(elapsed, 1),
                        "speed_kbps": 0, "blocks": block_num - 1,
                        "error": f"Overall timeout ({overall_timeout:.0f}s)"}

            data = f.read(BLOCK_SIZE)
            if not data:
                break

            # Pad last block with 0xFF
            if len(data) < BLOCK_SIZE:
                data += b'\xFF' * (BLOCK_SIZE - len(data))

            # Build packet: SOH + seq + ~seq + data + CRC16
            seq = block_num & 0xFF
            crc = crc16_ccitt(data)
            packet = bytes([SOH, seq, 0xFF - seq]) + data + struct.pack(">H", crc)

            # Send with retries
            port.timeout = PER_BLOCK_ACK_TIMEOUT
            for attempt in range(MAX_BLOCK_RETRIES):
                port.write(packet)
                port.flush()

                resp = port.read(1)
                if resp and resp[0] == ACK:
                    break
                elif resp and resp[0] == CAN:
                    elapsed = time.monotonic() - start_time
                    offset = (block_num - 1) * BLOCK_SIZE
                    return {"success": False,
                            "bytes_sent": offset,
                            "elapsed_seconds": round(elapsed, 1),
                            "speed_kbps": 0, "blocks": block_num - 1,
                            "error": f"Flasher cancelled at block {block_num}"}
                elif not resp:
                    # Timeout — no ACK or NAK
                    if attempt == MAX_BLOCK_RETRIES - 1:
                        elapsed = time.monotonic() - start_time
                        offset = (block_num - 1) * BLOCK_SIZE
                        return {
                            "success": False,
                            "bytes_sent": offset,
                            "elapsed_seconds": round(elapsed, 1),
                            "speed_kbps": 0, "blocks": block_num - 1,
                            "error": (f"Flasher stopped responding at block "
                                      f"{block_num} (offset 0x{offset:X})"),
                        }
                else:
                    # NAK or unknown — retry
                    if attempt == MAX_BLOCK_RETRIES - 1:
                        elapsed = time.monotonic() - start_time
                        offset = (block_num - 1) * BLOCK_SIZE
                        return {
                            "success": False,
                            "bytes_sent": offset,
                            "elapsed_seconds": round(elapsed, 1),
                            "speed_kbps": 0, "blocks": block_num - 1,
                            "error": (f"No ACK after {MAX_BLOCK_RETRIES} "
                                      f"retries at block {block_num}"),
                        }

            # Progress at 10% intervals
            bytes_sent = block_num * BLOCK_SIZE
            pct = min(100, bytes_sent * 100 // file_size)
            pct_bucket = pct // 10
            if pct_bucket > last_progress_pct:
                last_progress_pct = pct_bucket
                elapsed = time.monotonic() - start_time
                speed = bytes_sent / elapsed if elapsed > 0 else 0
                logger.info("XMODEM progress: %d%% (%d/%d KB, %.1f KB/s)",
                            pct, bytes_sent // 1024, file_size // 1024,
                            speed / 1024)
                if progress_callback:
                    progress_callback(bytes_sent, file_size, elapsed)

            block_num += 1

    # --- Phase 3: Send EOT ---
    port.timeout = PER_BLOCK_ACK_TIMEOUT
    eot_acked = False
    for _ in range(5):
        port.write(bytes([EOT]))
        port.flush()
        resp = port.read(1)
        if resp and resp[0] == ACK:
            eot_acked = True
            break

    elapsed = time.monotonic() - start_time
    speed = file_size / elapsed if elapsed > 0 else 0
    blocks_sent = block_num - 1

    logger.info("XMODEM transfer complete: %d bytes in %.1fs (%.1f KB/s)",
                file_size, elapsed, speed / 1024)

    if not eot_acked:
        return {"success": False, "bytes_sent": file_size,
                "elapsed_seconds": round(elapsed, 1),
                "speed_kbps": round(speed / 1024, 1),
                "blocks": blocks_sent,
                "error": "EOT not acknowledged"}

    # --- Phase 4: Read completion ---
    port.timeout = 1  # short read timeout for polling
    completion = read_completion(port, timeout=POST_EOT_TIMEOUT)

    result = {
        "success": completion["success"],
        "bytes_sent": file_size,
        "elapsed_seconds": round(elapsed, 1),
        "speed_kbps": round(speed / 1024, 1),
        "blocks": blocks_sent,
        "flasher_message": completion["message"],
    }
    if not completion["success"]:
        result["error"] = completion["message"]

    return result


def calculate_timeout(file_size: int) -> int:
    """Calculate overall timeout for a file: (size / 30000) * 2, min 60s."""
    return max(60, int((file_size / 30000) * 2))
