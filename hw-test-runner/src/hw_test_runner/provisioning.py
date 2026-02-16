"""WiFi provisioning protocol over BLE.

Implements the GATT protocol used by the wifi_prov Zephyr service.
"""

import asyncio
import struct
import logging
from typing import Optional

from bleak import BleakClient, BleakScanner

logger = logging.getLogger(__name__)

# UUID base: a0e4f2b0-XXXX-4c9a-b000-d0e6a7b8c9d0
UUID_SVC = "a0e4f2b0-0001-4c9a-b000-d0e6a7b8c9d0"
UUID_SCAN_TRIG = "a0e4f2b0-0002-4c9a-b000-d0e6a7b8c9d0"
UUID_SCAN_RES = "a0e4f2b0-0003-4c9a-b000-d0e6a7b8c9d0"
UUID_CRED = "a0e4f2b0-0004-4c9a-b000-d0e6a7b8c9d0"
UUID_STATUS = "a0e4f2b0-0005-4c9a-b000-d0e6a7b8c9d0"
UUID_RESET = "a0e4f2b0-0006-4c9a-b000-d0e6a7b8c9d0"

SECURITY_NAMES = {
    0: "Open",
    1: "WPA-PSK",
    2: "WPA2-PSK",
    3: "WPA2-PSK-SHA256",
    4: "WPA3-SAE",
}

SECURITY_BY_NAME = {v.lower(): k for k, v in SECURITY_NAMES.items()}

STATE_NAMES = {
    0: "IDLE",
    1: "SCANNING",
    2: "SCAN_COMPLETE",
    3: "PROVISIONING",
    4: "CONNECTING",
    5: "CONNECTED",
}


def decode_scan_result(data: bytes) -> Optional[dict]:
    """Decode a scan result from wire format."""
    if len(data) < 4:
        return None
    ssid_len = data[0]
    if len(data) < 1 + ssid_len + 3:
        return None
    ssid = data[1 : 1 + ssid_len].decode("utf-8", errors="replace")
    offset = 1 + ssid_len
    rssi = struct.unpack_from("b", data, offset)[0]
    security = data[offset + 1]
    channel = data[offset + 2]
    return {
        "ssid": ssid,
        "rssi": rssi,
        "security": SECURITY_NAMES.get(security, f"Unknown({security})"),
        "channel": channel,
    }


def encode_credentials(ssid: str, psk: str, security: int) -> bytes:
    """Encode credentials to wire format."""
    ssid_bytes = ssid.encode("utf-8")[:32]
    psk_bytes = psk.encode("utf-8")[:64]
    return (
        bytes([len(ssid_bytes)])
        + ssid_bytes
        + bytes([len(psk_bytes)])
        + psk_bytes
        + bytes([security])
    )


def decode_status(data: bytes) -> dict:
    """Decode status response from wire format."""
    if len(data) < 1:
        return {"state": "UNKNOWN", "raw": data.hex()}
    state = data[0]
    result = {"state": STATE_NAMES.get(state, f"Unknown({state})")}
    if state == 5 and len(data) >= 5:  # CONNECTED
        ip_bytes = data[1:5]
        result["ip"] = ".".join(str(b) for b in ip_bytes)
    return result


async def find_device(
    address: Optional[str] = None,
    timeout: float = 5.0,
) -> Optional[str]:
    """Find a device advertising the provisioning service. Returns address."""
    scanner = BleakScanner(service_uuids=[UUID_SVC])
    await scanner.start()
    await asyncio.sleep(timeout)
    await scanner.stop()

    devices = list(scanner.discovered_devices_and_advertisement_data.values())
    if address:
        for d, _adv in devices:
            if d.address.upper() == address.upper():
                return d.address
        return None
    if devices:
        return devices[0][0].address
    return None


async def scan_aps(
    address: Optional[str] = None,
    timeout: float = 15.0,
) -> list[dict]:
    """Trigger WiFi AP scan and return results."""
    device_addr = await find_device(address, timeout=5.0)
    if not device_addr:
        raise RuntimeError("No provisioning device found")

    results = []

    async with BleakClient(device_addr, timeout=timeout) as client:
        event = asyncio.Event()

        def on_scan_result(_sender, data: bytearray):
            result = decode_scan_result(bytes(data))
            if result:
                results.append(result)

        await client.start_notify(UUID_SCAN_RES, on_scan_result)
        await client.write_gatt_char(UUID_SCAN_TRIG, b"\x01", response=True)

        # Wait for scan results (device sends them then stops)
        await asyncio.sleep(min(timeout, 10.0))
        await client.stop_notify(UUID_SCAN_RES)

    results.sort(key=lambda x: x.get("rssi", -999), reverse=True)
    return results


async def provision(
    ssid: str,
    psk: str,
    security: Optional[str] = None,
    address: Optional[str] = None,
    timeout: float = 30.0,
) -> dict:
    """Send WiFi credentials and wait for connection status."""
    device_addr = await find_device(address, timeout=5.0)
    if not device_addr:
        raise RuntimeError("No provisioning device found")

    sec_code = SECURITY_BY_NAME.get((security or "wpa2-psk").lower(), 2)
    cred_data = encode_credentials(ssid, psk, sec_code)

    async with BleakClient(device_addr, timeout=timeout) as client:
        # Write credentials
        await client.write_gatt_char(UUID_CRED, cred_data, response=True)

        # Poll status until connected or timeout
        for _ in range(int(timeout / 2)):
            await asyncio.sleep(2.0)
            status_data = await client.read_gatt_char(UUID_STATUS)
            status = decode_status(bytes(status_data))
            logger.info("Provisioning status: %s", status)
            if status["state"] == "CONNECTED":
                return {"success": True, **status}
            if status["state"] == "IDLE":
                return {"success": False, "state": "IDLE", "error": "Connection failed"}

    return {"success": False, "state": "TIMEOUT", "error": "Timed out waiting for connection"}


async def get_status(
    address: Optional[str] = None,
    timeout: float = 10.0,
) -> dict:
    """Query current provisioning/connection status."""
    device_addr = await find_device(address, timeout=5.0)
    if not device_addr:
        raise RuntimeError("No provisioning device found")

    async with BleakClient(device_addr, timeout=timeout) as client:
        data = await client.read_gatt_char(UUID_STATUS)
        return decode_status(bytes(data))


async def factory_reset(
    address: Optional[str] = None,
    timeout: float = 10.0,
) -> dict:
    """Send factory reset command."""
    device_addr = await find_device(address, timeout=5.0)
    if not device_addr:
        raise RuntimeError("No provisioning device found")

    async with BleakClient(device_addr, timeout=timeout) as client:
        await client.write_gatt_char(UUID_RESET, b"\xff", response=True)
        return {"success": True, "message": "Factory reset sent"}
