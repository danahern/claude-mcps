"""TCP throughput testing.

Implements upload/download/echo tests against a throughput server.
Protocol: 1-byte command prefix (0x01=echo, 0x02=sink, 0x03=source).
"""

import asyncio
import logging
import time
from typing import Optional

logger = logging.getLogger(__name__)

CMD_ECHO = 0x01
CMD_SINK = 0x02
CMD_SOURCE = 0x03

DEFAULT_PORT = 4242
DEFAULT_DURATION = 10.0
DEFAULT_BLOCK_SIZE = 1024


async def tcp_throughput(
    host: str,
    mode: str,
    port: int = DEFAULT_PORT,
    duration: float = DEFAULT_DURATION,
    block_size: int = DEFAULT_BLOCK_SIZE,
) -> dict:
    """Run a TCP throughput test.

    Args:
        host: Target IP address
        mode: "upload", "download", or "echo"
        port: TCP port
        duration: Test duration in seconds
        block_size: Data block size in bytes

    Returns:
        Dict with bytes_transferred, duration_s, throughput_kbps, etc.
    """
    reader, writer = await asyncio.open_connection(host, port)

    # Send command byte
    cmd = {"upload": CMD_SINK, "download": CMD_SOURCE, "echo": CMD_ECHO}[mode]
    writer.write(bytes([cmd]))
    await writer.drain()

    data_block = bytes(range(256)) * (block_size // 256 + 1)
    data_block = data_block[:block_size]

    bytes_sent = 0
    bytes_received = 0
    start = time.monotonic()
    deadline = start + duration

    try:
        if mode == "upload":
            while time.monotonic() < deadline:
                writer.write(data_block)
                await writer.drain()
                bytes_sent += len(data_block)

        elif mode == "download":
            while time.monotonic() < deadline:
                data = await asyncio.wait_for(
                    reader.read(block_size), timeout=max(0.1, deadline - time.monotonic())
                )
                if not data:
                    break
                bytes_received += len(data)

        elif mode == "echo":
            while time.monotonic() < deadline:
                writer.write(data_block)
                await writer.drain()
                bytes_sent += len(data_block)

                data = await asyncio.wait_for(
                    reader.read(block_size), timeout=max(0.1, deadline - time.monotonic())
                )
                if not data:
                    break
                bytes_received += len(data)

    except (asyncio.TimeoutError, ConnectionError) as e:
        logger.warning("Connection ended: %s", e)
    finally:
        writer.close()
        try:
            await writer.wait_closed()
        except Exception:
            pass

    elapsed = time.monotonic() - start
    total_bytes = bytes_sent + bytes_received
    throughput_kbps = (total_bytes * 8) / (elapsed * 1000) if elapsed > 0 else 0

    return {
        "mode": mode,
        "host": host,
        "port": port,
        "bytes_sent": bytes_sent,
        "bytes_received": bytes_received,
        "duration_s": round(elapsed, 2),
        "throughput_kbps": round(throughput_kbps, 1),
        "block_size": block_size,
    }
