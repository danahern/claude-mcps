"""Unit tests for analysis.py — no hardware or Logic 2 required."""

from saleae_logic.analysis import (
    analyze_i2c_data,
    analyze_spi_data,
    analyze_uart_data,
    compute_timing_info,
    search_csv_data,
)

# ── I2C ──────────────────────────────────────────────────────────────

I2C_CSV = """\
start_time,address,data,ack
0.001,0x48,0xE1,ACK
0.002,0x48,0x00,ACK
0.003,0x68,0x75,NAK
0.010,0x76,0x88,ACK
0.015,0x76,0xAC,ACK
"""


def test_analyze_i2c_basic():
    result = analyze_i2c_data(I2C_CSV)
    assert result["total_transactions"] == 5
    assert set(result["addresses_seen"]) == {"0x48", "0x68", "0x76"}
    assert result["nak_count"] == 1
    assert result["duration_ms"] is not None
    assert result["duration_ms"] > 0


def test_analyze_i2c_empty():
    result = analyze_i2c_data("start_time,address,data,ack\n")
    assert result["total_transactions"] == 0


# ── SPI ──────────────────────────────────────────────────────────────

SPI_CSV = """\
start_time,mosi,miso
0.001,0x9F,0xFF
0.002,0x00,0xEF
0.003,0x00,0x40
0.004,0x00,0x18
"""


def test_analyze_spi_basic():
    result = analyze_spi_data(SPI_CSV)
    assert result["total_transfers"] == 4
    assert result["bytes_transferred"] == 8  # 4 MOSI + 4 MISO


def test_analyze_spi_empty():
    result = analyze_spi_data("start_time,mosi,miso\n")
    assert result["total_transfers"] == 0


# ── UART ─────────────────────────────────────────────────────────────

UART_CSV = """\
start_time,data,error
0.001,0x42,
0.002,0x6F,
0.003,0x6F,
0.004,0x74,
0.005,0x0A,
"""


def test_analyze_uart_basic():
    result = analyze_uart_data(UART_CSV)
    assert result["total_bytes"] == 5
    assert result["framing_errors"] == 0
    assert result["text_preview"] is not None
    assert "Boot" in result["text_preview"]


def test_analyze_uart_with_errors():
    csv_data = """\
start_time,data,framing_error
0.001,0x41,
0.002,0x42,Framing Error
0.003,0x43,
"""
    result = analyze_uart_data(csv_data)
    assert result["total_bytes"] == 3
    assert result["framing_errors"] == 1


# ── Search ───────────────────────────────────────────────────────────


def test_search_csv_data_basic():
    matches = search_csv_data(I2C_CSV, pattern="0x48")
    assert len(matches) == 2
    for m in matches:
        assert "address" in m
        assert m["address"] == "0x48"


def test_search_csv_data_column_filter():
    matches = search_csv_data(I2C_CSV, pattern="0x48", column="address")
    assert len(matches) == 2


def test_search_csv_data_no_match():
    matches = search_csv_data(I2C_CSV, pattern="0xFF")
    assert len(matches) == 0


def test_search_csv_data_regex():
    matches = search_csv_data(I2C_CSV, pattern="0x7[0-9a-fA-F]")
    # Should match 0x75, 0x76
    assert len(matches) >= 2


def test_search_csv_data_max_results():
    matches = search_csv_data(I2C_CSV, pattern="0x", max_results=2)
    assert len(matches) == 2


# ── Timing ───────────────────────────────────────────────────────────

TIMING_CSV = """\
time,channel_0
0.000,0
0.001,1
0.002,0
0.003,1
0.004,0
0.005,1
0.006,0
"""


def test_compute_timing_basic():
    result = compute_timing_info(TIMING_CSV, channel=0)
    assert result["channel"] == 0
    assert result["total_edges"] == 6  # 3 rising + 3 falling
    assert result["rising_edges"] == 3
    assert result["falling_edges"] == 3
    assert abs(result["frequency_hz"] - 500.0) < 1.0  # 0.002s period = 500 Hz
    assert abs(result["duty_cycle_percent"] - 50.0) < 1.0


def test_compute_timing_constant_signal():
    csv_data = """\
time,channel_0
0.000,1
0.001,1
0.002,1
"""
    result = compute_timing_info(csv_data, channel=0)
    assert result["edge_count"] == 0
    assert result["constant_value"] == 1


def test_compute_timing_asymmetric():
    # 75% duty cycle: high for 0.003s, low for 0.001s
    csv_data = """\
time,channel_0
0.000,0
0.001,1
0.004,0
0.005,1
0.008,0
"""
    result = compute_timing_info(csv_data, channel=0)
    assert result["total_edges"] == 4
    assert abs(result["duty_cycle_percent"] - 75.0) < 1.0


def test_compute_timing_insufficient_data():
    csv_data = "time,channel_0\n0.000,0\n"
    result = compute_timing_info(csv_data, channel=0)
    assert "error" in result
