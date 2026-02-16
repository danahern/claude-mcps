# hw-test-runner

Hardware test runner MCP server for BLE and TCP testing. Decouples BLE/WiFi testing from probe-rs, eliminating the disconnect/reconnect cycle during hardware verification.

## Setup

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -e ".[dev]"
```

Requires macOS with Bluetooth enabled (uses CoreBluetooth via bleak).

## Tools by Category

### Low-Level BLE
- `ble_discover(service_uuid?, timeout?)` — Scan for BLE devices, optionally filter by service UUID
- `ble_read(address, characteristic_uuid)` — Connect, read a GATT characteristic, disconnect
- `ble_write(address, characteristic_uuid, data)` — Connect, write hex data to a characteristic, disconnect
- `ble_subscribe(address, characteristic_uuid, timeout?)` — Subscribe to notifications, collect for timeout period

### WiFi Provisioning (High-Level)
- `wifi_provision(ssid, psk, security?, address?, timeout?)` — Full provisioning flow: discover device, send credentials, wait for connection
- `wifi_scan_aps(address?, timeout?)` — Trigger WiFi AP scan on device, return results
- `wifi_status(address?)` — Query WiFi connection status via BLE
- `wifi_factory_reset(address?)` — Send factory reset command

### TCP Throughput
- `tcp_throughput(host, mode, port?, duration?, block_size?)` — Run upload/download/echo throughput test

## Key Details

- Python MCP server using bleak for BLE (CoreBluetooth on macOS)
- WiFi provisioning implements the GATT protocol from the `wifi_prov` Zephyr service
- BLE operations are independent of probe-rs — no J-Link conflicts
- TCP throughput uses a 1-byte command prefix protocol (0x01=echo, 0x02=sink, 0x03=source)
- All tools return structured JSON responses

## Testing

```bash
.venv/bin/python -m pytest tests/ -v
```

Tests cover protocol encode/decode (provisioning wire format), tool definitions, and constants.
