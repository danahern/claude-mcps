"""Low-level BLE operations using bleak."""

import asyncio
import logging
from typing import Optional

from bleak import BleakClient, BleakScanner
from bleak.backends.device import BLEDevice

logger = logging.getLogger(__name__)


async def discover(
    service_uuid: Optional[str] = None,
    timeout: float = 5.0,
) -> list[dict]:
    """Scan for BLE devices. Optionally filter by advertised service UUID."""
    logger.info("BLE scan: service=%s timeout=%.1fs", service_uuid, timeout)

    scanner = BleakScanner(service_uuids=[service_uuid] if service_uuid else None)
    await scanner.start()
    await asyncio.sleep(timeout)
    await scanner.stop()

    results = []
    for d, adv in scanner.discovered_devices_and_advertisement_data.values():
        entry = {
            "name": d.name or "(unknown)",
            "address": d.address,
            "rssi": adv.rssi,
        }
        if adv.service_uuids:
            entry["services"] = adv.service_uuids
        results.append(entry)

    results.sort(key=lambda x: x.get("rssi", -999), reverse=True)
    return results


async def read_characteristic(
    address: str,
    characteristic_uuid: str,
    timeout: float = 10.0,
) -> bytes:
    """Connect, read a characteristic, disconnect."""
    async with BleakClient(address, timeout=timeout) as client:
        data = await client.read_gatt_char(characteristic_uuid)
        return bytes(data)


async def write_characteristic(
    address: str,
    characteristic_uuid: str,
    data: bytes,
    response: bool = True,
    timeout: float = 10.0,
) -> None:
    """Connect, write a characteristic, disconnect."""
    async with BleakClient(address, timeout=timeout) as client:
        await client.write_gatt_char(characteristic_uuid, data, response=response)


async def subscribe_notifications(
    address: str,
    characteristic_uuid: str,
    timeout: float = 10.0,
    max_notifications: int = 100,
) -> list[bytes]:
    """Connect, subscribe to notifications, collect until timeout or max count."""
    collected: list[bytes] = []
    event = asyncio.Event()

    def callback(_sender, data: bytearray):
        collected.append(bytes(data))
        if len(collected) >= max_notifications:
            event.set()

    async with BleakClient(address, timeout=timeout) as client:
        await client.start_notify(characteristic_uuid, callback)
        try:
            await asyncio.wait_for(event.wait(), timeout=timeout)
        except asyncio.TimeoutError:
            pass
        await client.stop_notify(characteristic_uuid)

    return collected
