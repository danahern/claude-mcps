"""CSV parsing and protocol data analysis helpers."""

import csv
import io
import re


def analyze_i2c_data(csv_content: str) -> dict:
    """Analyze I2C protocol data from exported CSV."""
    rows = list(csv.DictReader(io.StringIO(csv_content)))
    if not rows:
        return {"total_transactions": 0}

    addresses = set()
    nak_count = 0
    error_frames = []

    for i, row in enumerate(rows):
        # Look for address fields (varies by export format)
        for key in row:
            key_lower = key.lower()
            if "address" in key_lower and row[key].strip():
                addresses.add(row[key].strip())
            if "ack" in key_lower or "nak" in key_lower:
                val = row[key].strip().upper()
                if val in ("NAK", "NACK", "NAK/NACK", "false", "0"):
                    nak_count += 1
            if "error" in key_lower and row[key].strip():
                error_frames.append({"row": i, "error": row[key].strip()})

    # Compute timing from first/last row timestamps
    duration_ms = _compute_duration_ms(rows)

    return {
        "total_transactions": len(rows),
        "addresses_seen": sorted(addresses),
        "nak_count": nak_count,
        "error_frames": error_frames[:20],
        "duration_ms": duration_ms,
    }


def analyze_spi_data(csv_content: str) -> dict:
    """Analyze SPI protocol data from exported CSV."""
    rows = list(csv.DictReader(io.StringIO(csv_content)))
    if not rows:
        return {"total_transfers": 0}

    total_bytes = 0
    for row in rows:
        for key in row:
            key_lower = key.lower()
            if "mosi" in key_lower or "miso" in key_lower:
                val = row[key].strip()
                if val:
                    # Count bytes: hex values like "0xFF" = 1 byte
                    total_bytes += 1

    duration_ms = _compute_duration_ms(rows)

    return {
        "total_transfers": len(rows),
        "bytes_transferred": total_bytes,
        "duration_ms": duration_ms,
    }


def analyze_uart_data(csv_content: str) -> dict:
    """Analyze UART/Async Serial data from exported CSV."""
    rows = list(csv.DictReader(io.StringIO(csv_content)))
    if not rows:
        return {"total_bytes": 0}

    framing_errors = 0
    text_chars = []

    for row in rows:
        for key in row:
            key_lower = key.lower()
            if "error" in key_lower or "framing" in key_lower:
                val = row[key].strip()
                if val and val.lower() not in ("", "none", "no error"):
                    framing_errors += 1
            if "data" in key_lower:
                val = row[key].strip()
                if val:
                    # Try to interpret as character
                    try:
                        if val.startswith("0x") or val.startswith("0X"):
                            char_val = int(val, 16)
                        else:
                            char_val = int(val)
                        if 32 <= char_val <= 126:
                            text_chars.append(chr(char_val))
                        elif char_val == 10:
                            text_chars.append("\n")
                        elif char_val == 13:
                            text_chars.append("\r")
                    except (ValueError, OverflowError):
                        # Might already be ASCII text
                        text_chars.append(val)

    text_preview = "".join(text_chars[:500])
    duration_ms = _compute_duration_ms(rows)

    return {
        "total_bytes": len(rows),
        "framing_errors": framing_errors,
        "duration_ms": duration_ms,
        "text_preview": text_preview if text_preview else None,
    }


def search_csv_data(
    csv_content: str,
    pattern: str,
    column: str | None = None,
    max_results: int = 100,
) -> list[dict]:
    """Search CSV data for rows matching a regex pattern."""
    rows = list(csv.DictReader(io.StringIO(csv_content)))
    regex = re.compile(pattern, re.IGNORECASE)
    matches = []

    for i, row in enumerate(rows):
        if column:
            val = row.get(column, "")
            if regex.search(val):
                matches.append({"row": i, **row})
        else:
            for val in row.values():
                if regex.search(str(val)):
                    matches.append({"row": i, **row})
                    break
        if len(matches) >= max_results:
            break

    return matches


def compute_timing_info(csv_content: str, channel: int) -> dict:
    """Compute frequency, duty cycle, and pulse widths from raw digital CSV."""
    rows = list(csv.DictReader(io.StringIO(csv_content)))
    if len(rows) < 2:
        return {"error": "Not enough data points for timing analysis"}

    # Parse timestamps and values
    timestamps = []
    values = []
    time_key = None
    value_key = None

    if rows:
        for key in rows[0]:
            key_lower = key.lower()
            if "time" in key_lower:
                time_key = key
            elif str(channel) in key or "digital" in key_lower:
                value_key = key

    if not time_key or not value_key:
        # Fall back to positional
        headers = list(rows[0].keys())
        if len(headers) >= 2:
            time_key = headers[0]
            value_key = headers[1]
        else:
            return {"error": "Cannot identify time and value columns"}

    for row in rows:
        try:
            timestamps.append(float(row[time_key]))
            values.append(int(row[value_key]))
        except (ValueError, KeyError):
            continue

    if len(timestamps) < 2:
        return {"error": "Not enough valid data points"}

    # Find edges (transitions)
    edges = []
    for i in range(1, len(values)):
        if values[i] != values[i - 1]:
            edges.append({
                "time": timestamps[i],
                "type": "rising" if values[i] == 1 else "falling",
            })

    if not edges:
        return {
            "channel": channel,
            "constant_value": values[0] if values else None,
            "duration_seconds": timestamps[-1] - timestamps[0] if timestamps else 0,
            "edge_count": 0,
        }

    # Compute periods between same-type edges
    rising_times = [e["time"] for e in edges if e["type"] == "rising"]
    falling_times = [e["time"] for e in edges if e["type"] == "falling"]

    periods = []
    for i in range(1, len(rising_times)):
        periods.append(rising_times[i] - rising_times[i - 1])

    # Compute high/low durations for duty cycle
    high_durations = []
    low_durations = []
    for i in range(len(edges) - 1):
        dt = edges[i + 1]["time"] - edges[i]["time"]
        if edges[i]["type"] == "rising":
            high_durations.append(dt)
        else:
            low_durations.append(dt)

    result = {
        "channel": channel,
        "total_edges": len(edges),
        "rising_edges": len(rising_times),
        "falling_edges": len(falling_times),
        "duration_seconds": timestamps[-1] - timestamps[0],
    }

    if periods:
        avg_period = sum(periods) / len(periods)
        result["frequency_hz"] = 1.0 / avg_period if avg_period > 0 else 0
        result["average_period_seconds"] = avg_period
        result["min_period_seconds"] = min(periods)
        result["max_period_seconds"] = max(periods)

    if high_durations and low_durations:
        avg_high = sum(high_durations) / len(high_durations)
        avg_low = sum(low_durations) / len(low_durations)
        total = avg_high + avg_low
        if total > 0:
            result["duty_cycle_percent"] = (avg_high / total) * 100.0

    if high_durations:
        result["min_high_seconds"] = min(high_durations)
        result["max_high_seconds"] = max(high_durations)

    if low_durations:
        result["min_low_seconds"] = min(low_durations)
        result["max_low_seconds"] = max(low_durations)

    return result


def _compute_duration_ms(rows: list[dict]) -> float | None:
    """Extract duration from timestamp columns in first/last rows."""
    if not rows:
        return None

    time_key = None
    for key in rows[0]:
        if "time" in key.lower() or "start" in key.lower():
            time_key = key
            break

    if not time_key:
        return None

    try:
        first = float(rows[0][time_key])
        last = float(rows[-1][time_key])
        return round((last - first) * 1000, 3)
    except (ValueError, KeyError):
        return None
