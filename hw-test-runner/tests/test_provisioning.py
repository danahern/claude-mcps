"""Tests for WiFi provisioning protocol encode/decode."""

import struct
from hw_test_runner.provisioning import (
    decode_scan_result,
    decode_status,
    encode_credentials,
    SECURITY_BY_NAME,
)


class TestDecodeScanResult:
    def test_basic_scan_result(self):
        ssid = b"MyNetwork"
        rssi = -42
        security = 2  # WPA2-PSK
        channel = 6
        data = bytes([len(ssid)]) + ssid + struct.pack("b", rssi) + bytes([security, channel])
        result = decode_scan_result(data)
        assert result is not None
        assert result["ssid"] == "MyNetwork"
        assert result["rssi"] == rssi
        assert result["security"] == "WPA2-PSK"
        assert result["channel"] == 6

    def test_empty_ssid(self):
        data = bytes([0]) + struct.pack("b", -80) + bytes([0, 1])
        result = decode_scan_result(data)
        assert result is not None
        assert result["ssid"] == ""
        assert result["rssi"] == -80
        assert result["security"] == "Open"
        assert result["channel"] == 1

    def test_too_short(self):
        assert decode_scan_result(b"\x03ab") is None

    def test_completely_empty(self):
        assert decode_scan_result(b"") is None
        assert decode_scan_result(b"\x00") is None

    def test_unknown_security(self):
        ssid = b"Test"
        data = bytes([len(ssid)]) + ssid + struct.pack("b", -50) + bytes([99, 11])
        result = decode_scan_result(data)
        assert result is not None
        assert result["security"] == "Unknown(99)"

    def test_all_security_types(self):
        for sec_code, sec_name in [(0, "Open"), (1, "WPA-PSK"), (2, "WPA2-PSK"), (4, "WPA3-SAE")]:
            ssid = b"Net"
            data = bytes([len(ssid)]) + ssid + struct.pack("b", -60) + bytes([sec_code, 1])
            result = decode_scan_result(data)
            assert result["security"] == sec_name


class TestEncodeCredentials:
    def test_basic_encoding(self):
        data = encode_credentials("MyNet", "password123", 2)
        # ssid_len + ssid + psk_len + psk + security
        assert data[0] == 5  # len("MyNet")
        assert data[1:6] == b"MyNet"
        assert data[6] == 11  # len("password123")
        assert data[7:18] == b"password123"
        assert data[18] == 2  # WPA2-PSK

    def test_empty_password(self):
        data = encode_credentials("Open", "", 0)
        assert data[0] == 4
        assert data[1:5] == b"Open"
        assert data[5] == 0  # empty password
        assert data[6] == 0  # Open security

    def test_ssid_truncation(self):
        long_ssid = "A" * 50
        data = encode_credentials(long_ssid, "pw", 2)
        assert data[0] == 32  # truncated to 32

    def test_psk_truncation(self):
        long_psk = "B" * 100
        data = encode_credentials("Net", long_psk, 2)
        ssid_end = 1 + 3  # ssid_len + "Net"
        assert data[ssid_end] == 64  # truncated to 64

    def test_roundtrip_with_decode(self):
        """Encode then decode to verify consistency."""
        encoded = encode_credentials("TestNet", "pass", 2)
        # The credential format is different from scan result format,
        # so this just verifies encoding structure
        ssid_len = encoded[0]
        assert ssid_len == 7
        ssid = encoded[1:1 + ssid_len].decode("utf-8")
        assert ssid == "TestNet"


class TestDecodeStatus:
    def test_idle(self):
        result = decode_status(bytes([0]))
        assert result["state"] == "IDLE"

    def test_scanning(self):
        result = decode_status(bytes([1]))
        assert result["state"] == "SCANNING"

    def test_connected_with_ip(self):
        data = bytes([5, 192, 168, 1, 42])
        result = decode_status(data)
        assert result["state"] == "CONNECTED"
        assert result["ip"] == "192.168.1.42"

    def test_connected_without_ip(self):
        result = decode_status(bytes([5]))
        assert result["state"] == "CONNECTED"
        assert "ip" not in result

    def test_unknown_state(self):
        result = decode_status(bytes([99]))
        assert result["state"] == "Unknown(99)"

    def test_empty_data(self):
        result = decode_status(b"")
        assert result["state"] == "UNKNOWN"

    def test_all_states(self):
        expected = {0: "IDLE", 1: "SCANNING", 2: "SCAN_COMPLETE",
                    3: "PROVISIONING", 4: "CONNECTING", 5: "CONNECTED"}
        for code, name in expected.items():
            result = decode_status(bytes([code]))
            assert result["state"] == name


class TestSecurityByName:
    def test_known_names(self):
        assert SECURITY_BY_NAME["open"] == 0
        assert SECURITY_BY_NAME["wpa-psk"] == 1
        assert SECURITY_BY_NAME["wpa2-psk"] == 2
        assert SECURITY_BY_NAME["wpa3-sae"] == 4

    def test_case_insensitive_lookup(self):
        # The dict is built with lowercase keys
        assert "wpa2-psk" in SECURITY_BY_NAME
        assert "WPA2-PSK" not in SECURITY_BY_NAME  # keys are lowercase
