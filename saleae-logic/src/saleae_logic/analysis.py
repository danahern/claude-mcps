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


# ── Deep analysis (numpy/pandas) ────────────────────────────────────


def _get_numpy():
    """Lazy import numpy."""
    import numpy as np
    return np


def _get_pandas():
    """Lazy import pandas."""
    import pandas as pd
    return pd


def _stats(arr) -> dict:
    """Compute common statistics for a numpy array."""
    np = _get_numpy()
    if len(arr) == 0:
        return {}
    return {
        "count": len(arr),
        "mean": float(np.mean(arr)),
        "median": float(np.median(arr)),
        "std_dev": float(np.std(arr)),
        "min": float(np.min(arr)),
        "max": float(np.max(arr)),
        "p95": float(np.percentile(arr, 95)),
        "p99": float(np.percentile(arr, 99)),
    }


def _find_time_key(columns: list[str]) -> str | None:
    """Find the timestamp column in CSV headers."""
    for col in columns:
        if "time" in col.lower() or "start" in col.lower():
            return col
    return None


def deep_analyze_protocol(csv_content: str, protocol: str) -> dict:
    """Statistical analysis of decoded protocol data using pandas/numpy."""
    np = _get_numpy()
    pd = _get_pandas()

    df = pd.read_csv(io.StringIO(csv_content))
    if df.empty:
        return {"error": "No data rows", "protocol": protocol}

    result = {"protocol": protocol, "total_rows": len(df)}

    # Find timestamp column
    time_col = _find_time_key(list(df.columns))
    if time_col and pd.api.types.is_numeric_dtype(df[time_col]):
        timestamps = df[time_col].values
        if len(timestamps) >= 2:
            # Inter-transaction gaps
            gaps = np.diff(timestamps)
            result["timing"] = {
                "transaction_gaps": _stats(gaps),
                "total_duration_seconds": float(timestamps[-1] - timestamps[0]),
                "transactions_per_second": (
                    len(timestamps) / (timestamps[-1] - timestamps[0])
                    if timestamps[-1] > timestamps[0] else 0
                ),
            }

    # Protocol-specific analysis
    proto_lower = protocol.lower()
    if "i2c" in proto_lower:
        result.update(_deep_i2c(df))
    elif "uart" in proto_lower or "serial" in proto_lower or "async" in proto_lower:
        result.update(_deep_uart(df))
    elif "spi" in proto_lower:
        result.update(_deep_spi(df))

    # Error analysis (generic)
    error_cols = [c for c in df.columns if "error" in c.lower()]
    if error_cols:
        total_errors = 0
        for col in error_cols:
            errors = df[col].dropna()
            errors = errors[errors.str.strip().str.len() > 0]
            errors = errors[~errors.str.lower().isin(["none", "no error", ""])]
            total_errors += len(errors)
        result["error_rate_percent"] = round(
            total_errors / len(df) * 100, 2
        ) if len(df) > 0 else 0
        result["total_errors"] = total_errors

    return result


def _deep_i2c(df) -> dict:
    """I2C-specific deep analysis."""
    pd = _get_pandas()
    result = {}

    # Address frequency histogram
    addr_col = None
    for col in df.columns:
        if "address" in col.lower():
            addr_col = col
            break
    if addr_col:
        addr_counts = df[addr_col].dropna().value_counts()
        result["address_histogram"] = addr_counts.to_dict()

    # NAK rate per address
    ack_col = None
    for col in df.columns:
        if "ack" in col.lower() or "nak" in col.lower():
            ack_col = col
            break
    if addr_col and ack_col:
        nak_vals = {"NAK", "NACK", "NAK/NACK", "false", "0"}
        df_with_addr = df.dropna(subset=[addr_col, ack_col])
        nak_mask = df_with_addr[ack_col].str.strip().str.upper().isin(nak_vals)
        nak_by_addr = df_with_addr[nak_mask].groupby(addr_col).size()
        total_by_addr = df_with_addr.groupby(addr_col).size()
        nak_rate = (nak_by_addr / total_by_addr * 100).fillna(0).round(2)
        result["nak_rate_per_address"] = nak_rate.to_dict()

    # Read/write ratio
    rw_col = None
    for col in df.columns:
        if "read" in col.lower() or "write" in col.lower() or "r/w" in col.lower():
            rw_col = col
            break
    if rw_col:
        rw_counts = df[rw_col].dropna().value_counts()
        result["read_write_ratio"] = rw_counts.to_dict()

    return result


def _deep_uart(df) -> dict:
    """UART-specific deep analysis."""
    result = {}

    # Find data column
    data_col = None
    for col in df.columns:
        if "data" in col.lower():
            data_col = col
            break
    if not data_col:
        return result

    data_vals = df[data_col].dropna()
    if data_vals.empty:
        return result

    # Byte value histogram (top 20)
    byte_counts = data_vals.value_counts()
    result["byte_histogram"] = byte_counts.head(20).to_dict()

    # ASCII analysis
    printable = 0
    text_chars = []
    for val in data_vals:
        val = str(val).strip()
        try:
            if val.startswith("0x") or val.startswith("0X"):
                char_val = int(val, 16)
            else:
                char_val = int(val)
            if 32 <= char_val <= 126:
                printable += 1
                text_chars.append(chr(char_val))
            elif char_val in (10, 13):
                text_chars.append(chr(char_val))
        except (ValueError, OverflowError):
            text_chars.append(val)
            if len(val) == 1 and val.isprintable():
                printable += 1

    result["ascii_printable_ratio"] = round(
        printable / len(data_vals) * 100, 1
    ) if len(data_vals) > 0 else 0
    result["decoded_text"] = "".join(text_chars[:1000])

    # Framing errors
    error_cols = [c for c in df.columns if "error" in c.lower() or "framing" in c.lower()]
    framing_errors = 0
    for col in error_cols:
        errors = df[col].dropna()
        errors = errors[errors.str.strip().str.len() > 0]
        errors = errors[~errors.str.lower().isin(["none", "no error", ""])]
        framing_errors += len(errors)
    result["framing_errors"] = framing_errors

    return result


def _deep_spi(df) -> dict:
    """SPI-specific deep analysis."""
    result = {}

    # Count MOSI/MISO columns
    mosi_col = None
    miso_col = None
    for col in df.columns:
        if "mosi" in col.lower():
            mosi_col = col
        elif "miso" in col.lower():
            miso_col = col

    byte_count = 0
    if mosi_col:
        mosi_data = df[mosi_col].dropna()
        mosi_data = mosi_data[mosi_data.str.strip().str.len() > 0]
        byte_count += len(mosi_data)
        result["mosi_byte_histogram"] = mosi_data.value_counts().head(20).to_dict()
    if miso_col:
        miso_data = df[miso_col].dropna()
        miso_data = miso_data[miso_data.str.strip().str.len() > 0]
        byte_count += len(miso_data)
        result["miso_byte_histogram"] = miso_data.value_counts().head(20).to_dict()

    result["total_bytes_transferred"] = byte_count
    return result


def deep_analyze_digital(csv_content: str, channel: int) -> dict:
    """Statistical analysis of raw digital signal data using numpy."""
    np = _get_numpy()

    rows = list(csv.DictReader(io.StringIO(csv_content)))
    if len(rows) < 3:
        return {"error": "Not enough data points for deep analysis"}

    # Parse timestamps and values
    time_key = None
    value_key = None
    for key in rows[0]:
        key_lower = key.lower()
        if "time" in key_lower:
            time_key = key
        elif str(channel) in key or "digital" in key_lower:
            value_key = key

    if not time_key or not value_key:
        headers = list(rows[0].keys())
        if len(headers) >= 2:
            time_key, value_key = headers[0], headers[1]
        else:
            return {"error": "Cannot identify time and value columns"}

    timestamps = []
    values = []
    for row in rows:
        try:
            timestamps.append(float(row[time_key]))
            values.append(int(row[value_key]))
        except (ValueError, KeyError):
            continue

    ts = np.array(timestamps)
    vals = np.array(values)
    if len(ts) < 3:
        return {"error": "Not enough valid data points"}

    # Find edges
    edges_mask = np.diff(vals) != 0
    edge_times = ts[1:][edges_mask]
    edge_dirs = np.diff(vals)[edges_mask]  # +1 = rising, -1 = falling

    result = {
        "channel": channel,
        "total_samples": len(ts),
        "total_edges": len(edge_times),
        "duration_seconds": float(ts[-1] - ts[0]),
    }

    if len(edge_times) < 2:
        result["signal_state"] = "constant" if len(edge_times) == 0 else "single_edge"
        return result

    # Rising/falling edge times
    rising_times = edge_times[edge_dirs > 0]
    falling_times = edge_times[edge_dirs < 0]

    # Periods (rising-to-rising)
    if len(rising_times) >= 2:
        periods = np.diff(rising_times)
        result["frequency"] = _stats(1.0 / periods)
        result["period"] = _stats(periods)
        result["jitter_percent"] = round(
            float(np.std(periods) / np.mean(periods) * 100), 3
        ) if np.mean(periods) > 0 else 0

    # Duty cycle from high/low durations
    all_edge_times = edge_times
    all_edge_dirs = edge_dirs
    high_durs = []
    low_durs = []
    for i in range(len(all_edge_times) - 1):
        dt = all_edge_times[i + 1] - all_edge_times[i]
        if all_edge_dirs[i] > 0:  # rising → next edge = high time
            high_durs.append(dt)
        else:
            low_durs.append(dt)

    if high_durs:
        result["high_pulse_width"] = _stats(np.array(high_durs))
    if low_durs:
        result["low_pulse_width"] = _stats(np.array(low_durs))
    if high_durs and low_durs:
        avg_high = np.mean(high_durs)
        avg_low = np.mean(low_durs)
        total = avg_high + avg_low
        if total > 0:
            result["duty_cycle_percent"] = round(float(avg_high / total * 100), 2)

    # Edge density — divide capture into 10 time bins
    if len(edge_times) > 0:
        duration = ts[-1] - ts[0]
        if duration > 0:
            n_bins = min(10, len(edge_times))
            bin_edges = np.linspace(ts[0], ts[-1], n_bins + 1)
            counts, _ = np.histogram(edge_times, bins=bin_edges)
            result["edge_density_per_bin"] = counts.tolist()

    # Stability score: lower jitter = higher score (0-100)
    if "jitter_percent" in result:
        jitter = result["jitter_percent"]
        result["stability_score"] = round(max(0, 100 - jitter * 10), 1)

    return result


def deep_analyze_analog(csv_content: str) -> dict:
    """Statistical analysis of raw analog signal data using numpy."""
    np = _get_numpy()

    rows = list(csv.DictReader(io.StringIO(csv_content)))
    if len(rows) < 3:
        return {"error": "Not enough data points for deep analysis"}

    # Parse timestamps and values
    headers = list(rows[0].keys())
    time_key = headers[0]
    value_key = headers[1] if len(headers) >= 2 else None
    if not value_key:
        return {"error": "Cannot identify value column"}

    timestamps = []
    values = []
    for row in rows:
        try:
            timestamps.append(float(row[time_key]))
            values.append(float(row[value_key]))
        except (ValueError, KeyError):
            continue

    ts = np.array(timestamps)
    vals = np.array(values)
    if len(vals) < 3:
        return {"error": "Not enough valid data points"}

    result = {
        "total_samples": len(vals),
        "duration_seconds": float(ts[-1] - ts[0]),
        "basic_stats": {
            "min": float(np.min(vals)),
            "max": float(np.max(vals)),
            "mean": float(np.mean(vals)),
            "rms": float(np.sqrt(np.mean(vals**2))),
            "std_dev": float(np.std(vals)),
            "peak_to_peak": float(np.max(vals) - np.min(vals)),
        },
    }

    # Crest factor
    rms = result["basic_stats"]["rms"]
    if rms > 0:
        result["crest_factor"] = round(float(np.max(np.abs(vals)) / rms), 3)

    # Zero-crossing rate
    zero_crossings = np.sum(np.diff(np.sign(vals)) != 0)
    duration = ts[-1] - ts[0]
    if duration > 0:
        result["zero_crossing_rate_hz"] = float(zero_crossings / duration / 2)

    # FFT — dominant frequencies
    if len(vals) >= 16 and duration > 0:
        sample_rate = len(vals) / duration
        # Remove DC component
        vals_ac = vals - np.mean(vals)
        fft_vals = np.abs(np.fft.rfft(vals_ac))
        freqs = np.fft.rfftfreq(len(vals_ac), d=1.0 / sample_rate)
        # Skip DC bin
        fft_vals = fft_vals[1:]
        freqs = freqs[1:]
        if len(fft_vals) > 0:
            # Top 5 peaks
            peak_indices = np.argsort(fft_vals)[-5:][::-1]
            result["fft_peaks"] = [
                {
                    "frequency_hz": round(float(freqs[i]), 2),
                    "magnitude": round(float(fft_vals[i]), 4),
                }
                for i in peak_indices
                if fft_vals[i] > 0
            ]

            # Noise floor estimate (median of FFT magnitudes)
            result["noise_floor"] = round(float(np.median(fft_vals)), 4)

    return result
