"""Tests for ISP protocol: checksum, packet framing, response parsing."""

from alif_flash.isp import (
    calc_checksum,
    make_packet,
    read_response,
    CMD_START_ISP,
    CMD_STOP_ISP,
    CMD_DOWNLOAD_DATA,
    CMD_DOWNLOAD_DONE,
    CMD_BURN_MRAM,
    CMD_ACK,
    CMD_DATA_RESP,
    CMD_ENQUIRY,
    CMD_SET_MAINTENANCE,
    DATA_PER_CHUNK,
)


class TestChecksum:
    def test_empty(self):
        assert calc_checksum(b'') == 0

    def test_single_byte(self):
        assert calc_checksum(b'\x01') == 0xFF

    def test_sums_to_zero(self):
        """Appending checksum to data should make sum mod 256 == 0."""
        data = b'\x03\x00'  # length=3, cmd=START_ISP
        cksum = calc_checksum(data)
        assert (sum(data) + cksum) & 0xFF == 0

    def test_known_values(self):
        # START_ISP packet body before checksum: [0x03, 0x00]
        # sum = 3, checksum = 256 - 3 = 253
        assert calc_checksum(b'\x03\x00') == 253

    def test_full_range(self):
        data = bytes(range(256))
        cksum = calc_checksum(data)
        assert (sum(data) + cksum) & 0xFF == 0


class TestMakePacket:
    def test_start_isp(self):
        pkt = make_packet(CMD_START_ISP)
        assert len(pkt) == 3  # length + cmd + checksum
        assert pkt[0] == 3   # length byte
        assert pkt[1] == CMD_START_ISP
        assert (sum(pkt)) & 0xFF == 0  # checksum valid

    def test_stop_isp(self):
        pkt = make_packet(CMD_STOP_ISP)
        assert pkt[0] == 3
        assert pkt[1] == CMD_STOP_ISP
        assert (sum(pkt)) & 0xFF == 0

    def test_with_data(self):
        data = b'\x01\x02\x03'
        pkt = make_packet(CMD_ENQUIRY, data)
        assert pkt[0] == len(data) + 3  # length + cmd + data + checksum
        assert pkt[1] == CMD_ENQUIRY
        assert pkt[2:5] == data
        assert (sum(pkt)) & 0xFF == 0

    def test_burn_mram_payload(self):
        """BURN_MRAM carries 8 bytes: addr(4) + size(4) in little-endian."""
        import struct
        addr = 0x80002000
        size = 0x1000
        data = struct.pack('<II', addr, size)
        pkt = make_packet(CMD_BURN_MRAM, data)
        assert pkt[0] == len(data) + 3
        assert pkt[1] == CMD_BURN_MRAM
        # Verify addr/size in packet
        parsed_addr, parsed_size = struct.unpack('<II', pkt[2:10])
        assert parsed_addr == addr
        assert parsed_size == size
        assert (sum(pkt)) & 0xFF == 0

    def test_download_data_chunk(self):
        """DOWNLOAD_DATA: 2-byte LE sequence + up to 240 bytes data."""
        import struct
        seq_num = 42
        chunk = bytes(range(240))
        data = struct.pack('<H', seq_num) + chunk
        pkt = make_packet(CMD_DOWNLOAD_DATA, data)
        assert pkt[1] == CMD_DOWNLOAD_DATA
        seq_parsed = struct.unpack('<H', pkt[2:4])[0]
        assert seq_parsed == 42
        assert pkt[4:244] == chunk
        assert (sum(pkt)) & 0xFF == 0

    def test_set_maintenance(self):
        pkt = make_packet(CMD_SET_MAINTENANCE)
        assert pkt[1] == CMD_SET_MAINTENANCE
        assert (sum(pkt)) & 0xFF == 0


class FakeSerial:
    """Minimal serial mock for read_response tests."""

    def __init__(self, data: bytes):
        self._data = data
        self._pos = 0
        self.timeout = 2

    def read(self, n: int) -> bytes:
        chunk = self._data[self._pos:self._pos + n]
        self._pos += n
        return chunk


class TestReadResponse:
    def test_ack_response(self):
        # ACK packet: [length=3, cmd=0xFE, checksum]
        pkt = make_packet(CMD_ACK)
        ser = FakeSerial(pkt)
        cmd, data = read_response(ser)
        assert cmd == CMD_ACK
        assert data == b''

    def test_data_response(self):
        # DATA_RESP with 4 bytes of data
        payload = b'\x01\x02\x03\x04'
        pkt = make_packet(CMD_DATA_RESP, payload)
        ser = FakeSerial(pkt)
        cmd, data = read_response(ser)
        assert cmd == CMD_DATA_RESP
        assert data == payload

    def test_empty_read(self):
        ser = FakeSerial(b'')
        cmd, data = read_response(ser)
        assert cmd is None
        assert data == b''

    def test_short_length(self):
        ser = FakeSerial(b'\x01')  # length=1, too short
        cmd, data = read_response(ser)
        assert cmd is None

    def test_enquiry_response(self):
        """ENQUIRY response has 10+ bytes of device info."""
        info = bytes(10)  # 10 bytes, byte[9] = maintenance flag
        pkt = make_packet(CMD_DATA_RESP, info)
        ser = FakeSerial(pkt)
        cmd, data = read_response(ser)
        assert cmd == CMD_DATA_RESP
        assert len(data) == 10
        assert data[9] == 0  # not in maintenance


class TestConstants:
    def test_data_per_chunk(self):
        assert DATA_PER_CHUNK == 240

    def test_command_values(self):
        assert CMD_START_ISP == 0x00
        assert CMD_STOP_ISP == 0x01
        assert CMD_DOWNLOAD_DATA == 0x04
        assert CMD_DOWNLOAD_DONE == 0x05
        assert CMD_BURN_MRAM == 0x08
        assert CMD_ACK == 0xFE
        assert CMD_DATA_RESP == 0xFD
