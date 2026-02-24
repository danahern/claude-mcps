"""Alif SE-UART ISP protocol implementation.

Packet format: [length, cmd, data..., checksum]
All bytes including checksum must sum to 0 mod 256.
Data transfers use 240-byte chunks with 2-byte LE sequence numbers.
"""

import asyncio
import glob
import logging
import os
import struct
import subprocess
import tempfile
import time

import serial

logger = logging.getLogger(__name__)

# Protocol constants
BAUD_RATE = 57600
DATA_PER_CHUNK = 240

# Commands
CMD_START_ISP = 0x00
CMD_STOP_ISP = 0x01
CMD_DOWNLOAD_DATA = 0x04
CMD_DOWNLOAD_DONE = 0x05
CMD_BURN_MRAM = 0x08
CMD_RESET_DEVICE = 0x09
CMD_ENQUIRY = 0x0F
CMD_SET_MAINTENANCE = 0x16
CMD_ACK = 0xFE
CMD_DATA_RESP = 0xFD


def calc_checksum(data: bytes) -> int:
    """All bytes including checksum must sum to 0 mod 256."""
    return (0 - sum(data)) & 0xFF


def make_packet(cmd: int, data: bytes = b'') -> bytes:
    """Build ISP packet: [length, cmd, data..., checksum]."""
    payload = bytes([cmd]) + data
    length = len(payload) + 2  # +1 for length byte, +1 for checksum
    pkt = bytes([length]) + payload
    pkt += bytes([calc_checksum(pkt)])
    return pkt


def read_response(ser: serial.Serial, timeout: float = 2) -> tuple[int | None, bytes]:
    """Read one ISP response packet. Returns (cmd, data) or (None, b'')."""
    old_timeout = ser.timeout
    try:
        ser.timeout = timeout
    except serial.SerialException:
        return None, b''
    try:
        first = ser.read(1)
        if not first:
            return None, b''
        length = first[0]
        if length < 2:
            return None, b''
        rest = ser.read(length - 1)
        if len(rest) < 1:
            return None, b''
        cmd = rest[0]
        data = rest[1:-1] if len(rest) > 2 else b''
        return cmd, data
    except (serial.SerialException, OSError):
        return None, b''
    finally:
        try:
            ser.timeout = old_timeout
        except (serial.SerialException, OSError):
            pass


def send_cmd(ser: serial.Serial, cmd: int, data: bytes = b'',
             label: str = "", quiet: bool = False) -> tuple[bool, bytes]:
    """Send ISP command, read response. Returns (ok, resp_data)."""
    pkt = make_packet(cmd, data)
    ser.reset_input_buffer()
    ser.write(pkt)
    ser.flush()
    time.sleep(0.05)

    resp_cmd, resp_data = read_response(ser)
    ok = resp_cmd in (CMD_ACK, CMD_DATA_RESP)

    if not quiet:
        if resp_cmd == CMD_ACK:
            status = "ACK"
        elif resp_cmd == CMD_DATA_RESP:
            status = f"DATA ({len(resp_data)} bytes)"
        elif resp_cmd is not None:
            status = f"0x{resp_cmd:02X}"
        else:
            status = "no response"
        logger.info("%s: %s", label, status)

    return ok, resp_data


def find_se_uart() -> list[str]:
    """Find available USB serial ports (JLink VCOM and FTDI)."""
    ports = glob.glob("/dev/cu.usbmodem*") + glob.glob("/dev/cu.usbserial*")
    return sorted(ports)


def wait_for_replug(timeout_disappear: float = 30, timeout_total: float = 60) -> str | None:
    """Wait for USB serial port to disappear then reappear.

    Returns the port path on success, None on timeout.
    """
    start = time.time()

    # Wait for port to disappear
    while time.time() - start < timeout_disappear:
        ports = find_se_uart()
        if not ports:
            logger.info("Port disappeared — board unplugged")
            break
        time.sleep(0.3)
    else:
        logger.warning("Timed out waiting for port to disappear (%ds)", timeout_disappear)
        return None

    # Wait for port to reappear
    while time.time() - start < timeout_total:
        ports = find_se_uart()
        if ports:
            port = ports[0]
            logger.info("Port reappeared: %s", port)
            time.sleep(0.5)  # Let USB settle
            return port
        time.sleep(0.3)

    logger.warning("Timed out waiting for port to reappear (%ds total)", timeout_total)
    return None


def reset_via_jlink(device: str | None = None, interface: str = "SWD",
                    speed: int = 4000) -> dict:
    """Reset the board via JLink, triggering SE boot sequence.

    Connects to the A32 core and issues a reset. The SE re-enters
    ISP-responsive mode after the reset. JLink may report "Failed to halt
    CPU" which is expected — we only need the reset, not halting.
    """
    from .jlink import JLINK_EXE, DEVICE_RESET

    if device is None:
        device = DEVICE_RESET

    if not os.path.exists(JLINK_EXE):
        return {"success": False, "message": f"JLinkExe not found at {JLINK_EXE}"}

    with tempfile.NamedTemporaryFile(mode='w', suffix='.jlink', delete=False) as f:
        f.write("r\nsleep 100\nexit\n")
        script_path = f.name

    try:
        result = subprocess.run(
            [JLINK_EXE, "-device", device, "-if", interface, "-speed", str(speed),
             "-autoconnect", "1", "-NoGui", "1", "-CommanderScript", script_path],
            capture_output=True, text=True, timeout=15,
        )
        logger.info("JLink reset: rc=%d", result.returncode)
        # "Could not find core" = wrong device/no connection at all
        # "Failed to halt" is expected (SE controls power domain) — reset still works
        if "Could not find core" in result.stdout or "Cannot connect" in result.stdout:
            return {"success": False, "message": "JLink could not connect to target",
                    "stdout": result.stdout[-500:]}
        return {"success": True, "message": "Board reset via JLink",
                "stdout": result.stdout[-500:]}
    except subprocess.TimeoutExpired:
        return {"success": False, "message": "JLink command timed out"}
    finally:
        os.unlink(script_path)


def open_serial(port: str, retries: int = 3, retry_delay: float = 2) -> serial.Serial:
    """Open serial port with retries (port may disappear during power cycle)."""
    for attempt in range(retries):
        try:
            ser = serial.Serial(port, BAUD_RATE, timeout=2)
            time.sleep(0.1)
            try:
                while ser.in_waiting:
                    ser.read(ser.in_waiting)
                    time.sleep(0.05)
            except (serial.SerialException, OSError):
                ser.close()
                raise serial.SerialException(f"Port {port} opened but not ready")
            return ser
        except (serial.SerialException, OSError) as e:
            if attempt < retries - 1:
                logger.warning("Port not ready, retrying in %ds... (%s)", retry_delay, e)
                time.sleep(retry_delay)
            else:
                raise


def start_isp(ser: serial.Serial, retries: int = 3) -> tuple[bool, bytes]:
    """Send START_ISP with retries — handles stale data in buffer."""
    for attempt in range(retries):
        time.sleep(0.1)
        try:
            while ser.in_waiting:
                ser.read(ser.in_waiting)
                time.sleep(0.05)
        except (serial.SerialException, OSError):
            pass
        ok, data = send_cmd(ser, CMD_START_ISP, label="START_ISP",
                            quiet=(attempt < retries - 1))
        if ok:
            if attempt > 0:
                logger.info("START_ISP: ACK (attempt %d)", attempt + 1)
            return True, data
        if attempt < retries - 1:
            time.sleep(0.3)
    return False, b''


def probe(port: str) -> dict:
    """Check if the SE is responsive. Returns status dict."""
    ser = open_serial(port)
    try:
        ok, _ = start_isp(ser)
        if not ok:
            return {
                "responsive": False,
                "port": port,
                "message": "SE did not respond to START_ISP. Board may need a power cycle.",
            }
        result = {"responsive": True, "port": port, "isp_mode": True}

        ok2, data = send_cmd(ser, CMD_ENQUIRY, label="ENQUIRY")
        if ok2 and len(data) >= 10:
            result["maintenance_mode"] = bool(data[9])
            result["enquiry_data"] = data.hex()
        send_cmd(ser, CMD_STOP_ISP, label="STOP_ISP", quiet=True)
        return result
    finally:
        ser.close()


def enter_maintenance(port: str, do_wait_for_replug: bool = False,
                      jlink_reset: bool = False) -> dict:
    """Enter maintenance mode via ISP protocol.

    Flow: START_ISP -> SET_MAINTENANCE -> STOP_ISP -> RESET -> reconnect -> verify

    If jlink_reset=True, resets the board via JLink first (no manual power cycle needed).
    If do_wait_for_replug=True, waits for manual unplug/replug instead.
    """
    steps = []

    if jlink_reset:
        steps.append("resetting board via JLink...")
        r = reset_via_jlink()
        if not r["success"]:
            return {"success": False, "message": r["message"], "steps": steps}
        steps.append("JLink reset: OK")
        # Wait for SE to initialize and port to stabilize
        time.sleep(2)
        steps.append(f"using port: {port}")
    elif do_wait_for_replug:
        steps.append("waiting for unplug/replug...")
        new_port = wait_for_replug()
        if not new_port:
            return {
                "success": False,
                "message": "Timed out waiting for board replug.",
                "steps": steps,
            }
        port = new_port
        steps.append(f"port reappeared: {port}")

    ser = open_serial(port)
    try:
        # Phase 1: Set maintenance flag
        ok, _ = start_isp(ser)
        if not ok:
            return {
                "success": False,
                "message": "SE did not respond. Try: unplug/replug PRG_USB, then run within 2-3s.",
                "steps": steps,
            }
        steps.append("START_ISP: ACK")

        send_cmd(ser, CMD_SET_MAINTENANCE, label="SET_MAINTENANCE")
        steps.append("SET_MAINTENANCE: sent")

        send_cmd(ser, CMD_STOP_ISP, label="STOP_ISP")
        steps.append("STOP_ISP: sent")

        ser.write(make_packet(CMD_RESET_DEVICE))
        ser.flush()
        ser.close()
        steps.append("RESET_DEVICE: sent")

        # Phase 2: Wait for reboot and reconnect
        time.sleep(5)
        steps.append("waited 5s for reboot")

        ser = open_serial(port, retries=5, retry_delay=2)
        steps.append("reconnected")

        # Phase 3: Verify
        ok, _ = start_isp(ser)
        if not ok:
            return {
                "success": False,
                "message": "SE not responding after reset. Try power cycling.",
                "steps": steps,
            }
        steps.append("START_ISP (post-reset): ACK")

        ok, data = send_cmd(ser, CMD_ENQUIRY, label="ENQUIRY")
        if ok and len(data) >= 10 and data[9]:
            steps.append("ENQUIRY: maintenance=YES")
            send_cmd(ser, CMD_STOP_ISP, label="STOP_ISP", quiet=True)
            ser.close()
            return {"success": True, "maintenance_mode": True, "steps": steps}

        maint = data[9] if ok and len(data) >= 10 else "unknown"
        steps.append(f"ENQUIRY: maintenance={maint} (proceeding anyway)")
        send_cmd(ser, CMD_STOP_ISP, label="STOP_ISP", quiet=True)
        ser.close()
        return {"success": True, "maintenance_mode": False, "steps": steps,
                "message": "Maintenance flag not confirmed, but MRAM write may still work."}
    except Exception:
        ser.close()
        raise


def _write_segment(ser: serial.Serial, data: bytes, addr: int,
                    name: str, seg_label: str = "") -> dict:
    """Write one BURN_MRAM segment using an existing serial connection."""
    size = len(data)
    label = f"[{name}]{seg_label}"
    logger.info("%s %d bytes -> 0x%08X", label, size, addr)

    ok, _ = send_cmd(ser, CMD_BURN_MRAM,
                     struct.pack('<II', addr, size), "BURN_MRAM")
    if not ok:
        return {"success": False, "message": f"BURN_MRAM rejected at 0x{addr:08X}"}

    offset = 0
    chunk_num = 0
    total = (size + DATA_PER_CHUNK - 1) // DATA_PER_CHUNK
    t0 = time.time()

    while offset < size:
        chunk = data[offset:offset + DATA_PER_CHUNK]
        seq = struct.pack('<H', chunk_num)
        pkt = make_packet(CMD_DOWNLOAD_DATA, seq + chunk)
        ser.reset_input_buffer()
        ser.write(pkt)
        ser.flush()
        time.sleep(0.02)

        resp_cmd, _ = read_response(ser, timeout=1)
        if resp_cmd not in (CMD_ACK, CMD_DATA_RESP):
            status = f"0x{resp_cmd:02X}" if resp_cmd else "no response"
            logger.warning("%s chunk %d/%d: %s", label, chunk_num, total, status)
            return {"success": False, "usb_drop": resp_cmd is None,
                    "message": f"Chunk {chunk_num}/{total} failed: {status}",
                    "chunks_written": chunk_num, "bytes_written": offset}

        offset += len(chunk)
        chunk_num += 1
        if chunk_num % 100 == 0 or chunk_num == total:
            elapsed = time.time() - t0
            pct = 100 * offset // size
            logger.info("%s %d/%d (%d%%) [%.1fs]", label, chunk_num, total, pct, elapsed)

    send_cmd(ser, CMD_DOWNLOAD_DONE, label="DOWNLOAD_DONE")
    return {"success": True, "chunks": total, "elapsed": time.time() - t0}


# Max bytes per BURN_MRAM session. When a USB drop is detected mid-segment,
# the connection is reopened and transfer resumes from the next segment.
MAX_SEGMENT_SIZE = 256 * 1024


def write_image(port: str, path: str, addr: int) -> dict:
    """Write a single image to MRAM, splitting into segments with reconnect on USB drop."""
    with open(path, 'rb') as f:
        data = f.read()
    orig = len(data)
    pad = (16 - (orig % 16)) % 16
    data += b'\x00' * pad
    size = len(data)
    name = os.path.basename(path)

    logger.info("[%s] %d bytes (padded to %d) -> 0x%08X", name, orig, size, addr)

    t0 = time.time()
    total_chunks = 0
    seg_offset = 0
    seg_num = 0
    num_segments = (size + MAX_SEGMENT_SIZE - 1) // MAX_SEGMENT_SIZE

    ser = open_serial(port)
    ok, _ = start_isp(ser)
    if not ok:
        ser.close()
        return {"success": False, "file": name, "message": "START_ISP failed"}

    try:
        while seg_offset < size:
            seg_data = data[seg_offset:seg_offset + MAX_SEGMENT_SIZE]
            seg_addr = addr + seg_offset
            seg_label = f" seg {seg_num + 1}/{num_segments}" if num_segments > 1 else ""

            r = _write_segment(ser, seg_data, seg_addr, name, seg_label)
            if not r["success"]:
                if r.get("usb_drop"):
                    # USB dropped — close, wait, reconnect, retry this segment
                    logger.warning("USB drop detected, reconnecting...")
                    try:
                        ser.close()
                    except Exception:
                        pass
                    time.sleep(3)
                    ser = open_serial(port, retries=5, retry_delay=2)
                    ok, _ = start_isp(ser)
                    if not ok:
                        return {"success": False, "file": name,
                                "message": "Failed to reconnect after USB drop"}
                    logger.info("Reconnected, retrying segment %d", seg_num + 1)
                    continue  # Retry same segment
                return {"success": False, "file": name, "message": r["message"]}

            total_chunks += r["chunks"]
            seg_offset += len(seg_data)
            seg_num += 1
    finally:
        try:
            send_cmd(ser, CMD_STOP_ISP, label="STOP_ISP", quiet=True)
        except (serial.SerialException, OSError):
            pass
        try:
            ser.close()
        except (serial.SerialException, OSError):
            pass

    elapsed = time.time() - t0
    return {
        "success": True,
        "file": name,
        "address": f"0x{addr:08X}",
        "original_bytes": orig,
        "padded_bytes": size,
        "chunks": total_chunks,
        "segments": num_segments,
        "elapsed_seconds": round(elapsed, 1),
        "bytes_per_second": round(size / elapsed) if elapsed > 0 else 0,
    }


def flash_images(port: str, config_path: str, enter_maint: bool = False,
                  do_wait_for_replug: bool = False, jlink_reset: bool = False) -> dict:
    """Flash ATOC package and all images defined in the ATOC JSON config."""
    import json

    if enter_maint:
        maint_result = enter_maintenance(
            port, do_wait_for_replug=do_wait_for_replug, jlink_reset=jlink_reset)
        if not maint_result["success"]:
            return {"success": False, "message": "Failed to enter maintenance mode",
                    "maintenance": maint_result}
        time.sleep(1)

    with open(config_path) as f:
        config = json.load(f)

    build_dir = os.path.normpath(os.path.join(os.path.dirname(config_path), ".."))
    images_dir = os.path.join(build_dir, "images")

    # ATOC goes at end of APP MRAM, just below System MRAM base
    # Address = System MRAM Base - ATOC file size
    SYSTEM_MRAM_BASE = 0x80580000
    atoc_path = os.path.join(build_dir, "AppTocPackage.bin")

    images = []
    if os.path.exists(atoc_path):
        atoc_size = os.path.getsize(atoc_path)
        atoc_addr = SYSTEM_MRAM_BASE - atoc_size
        logger.info("ATOC: %d bytes -> 0x%08X", atoc_size, atoc_addr)
        images.append((atoc_path, atoc_addr))
    else:
        logger.warning("AppTocPackage.bin not found at %s — run gen_toc first", atoc_path)

    for key, entry in config.items():
        if key == "DEVICE" or not isinstance(entry, dict):
            continue
        if entry.get("disabled", False):
            continue
        binary = entry.get("binary")
        addr_str = entry.get("mramAddress")
        if binary and addr_str:
            path = os.path.join(images_dir, binary)
            addr = int(addr_str, 16)
            if not os.path.exists(path):
                return {"success": False, "message": f"Image not found: {path}"}
            images.append((path, addr))

    if not images:
        return {"success": False, "message": "No images found in config"}

    total_bytes = sum(os.path.getsize(p) for p, _ in images)

    t0 = time.time()
    results = []
    for path, addr in images:
        r = write_image(port, path, addr)
        results.append(r)
        if not r["success"]:
            return {"success": False, "message": f"Failed writing {r['file']}",
                    "images": results}

    total_time = time.time() - t0

    # Final reset
    ser = open_serial(port)
    try:
        start_isp(ser)
        send_cmd(ser, CMD_STOP_ISP, label="STOP_ISP")
        ser.write(make_packet(CMD_RESET_DEVICE))
        ser.flush()
    finally:
        ser.close()

    return {
        "success": True,
        "total_bytes": total_bytes,
        "total_seconds": round(total_time, 1),
        "image_count": len(images),
        "images": results,
        "message": "All images written. Power cycle (unplug/replug PRG_USB) for A32 to boot.",
    }


def gen_toc(setools_dir: str, config_rel: str) -> dict:
    """Run app-gen-toc to generate ATOC package."""
    import subprocess

    app_gen_toc = os.path.join(setools_dir, "app-gen-toc")
    if not os.path.exists(app_gen_toc):
        return {"success": False, "message": f"app-gen-toc not found at {app_gen_toc}"}

    result = subprocess.run(
        ["./app-gen-toc", "-f", config_rel],
        cwd=setools_dir, capture_output=True, text=True, timeout=60,
    )
    if result.returncode != 0:
        return {"success": False, "message": f"app-gen-toc failed:\n{result.stderr}",
                "stdout": result.stdout}

    return {"success": True, "stdout": result.stdout, "stderr": result.stderr}


def monitor(port: str, baud: int = 115200, duration: float = 15,
            do_wait_for_replug: bool = False) -> dict:
    """Read serial console output. Optionally wait for board power cycle first."""
    if do_wait_for_replug:
        new_port = wait_for_replug()
        if not new_port:
            return {"success": False, "message": "Timed out waiting for port to reappear"}
        port = new_port

    ser = serial.Serial(port, baud, timeout=1)
    try:
        # Drain stale data
        while ser.in_waiting:
            ser.read(ser.in_waiting)
            time.sleep(0.05)

        buf = b''
        t0 = time.time()
        while time.time() - t0 < duration:
            data = ser.read(ser.in_waiting or 1)
            if data:
                buf += data

        text = buf.decode('utf-8', errors='replace')
        return {
            "success": True,
            "bytes": len(buf),
            "baud": baud,
            "port": port,
            "duration_seconds": round(time.time() - t0, 1),
            "output": text,
        }
    finally:
        ser.close()
