"""Tests for J-Link MRAM programming: output parsing, setup checks, script generation."""

import os
import tempfile

from alif_flash.jlink import (
    MRAM_LAYOUT,
    ATOC_KEY_MAP,
    _parse_loadbin_output,
    check_setup,
    JLINK_DEVICES_DIR,
)


class TestParseLoadbinOutput:
    def test_single_success(self):
        stdout = (
            "Downloading file [/tmp/bl32.bin]...\n"
            "O.K.\n"
        )
        results = _parse_loadbin_output(stdout)
        assert len(results) == 1
        assert results[0] == {"file": "bl32.bin", "success": True}

    def test_multiple_success(self):
        stdout = (
            "Downloading file [/path/to/bl32.bin]...\n"
            "O.K.\n"
            "Downloading file [/path/to/xipImage]...\n"
            "O.K.\n"
            "Downloading file [/path/to/cramfs-xip.img]...\n"
            "O.K.\n"
        )
        results = _parse_loadbin_output(stdout)
        assert len(results) == 3
        assert all(r["success"] for r in results)
        assert results[0]["file"] == "bl32.bin"
        assert results[1]["file"] == "xipImage"
        assert results[2]["file"] == "cramfs-xip.img"

    def test_failure(self):
        stdout = (
            "Downloading file [/tmp/bl32.bin]...\n"
            "Writing target memory failed.\n"
        )
        results = _parse_loadbin_output(stdout)
        assert len(results) == 1
        assert results[0]["success"] is False
        assert "failed" in results[0]["error"].lower()

    def test_mixed(self):
        stdout = (
            "Downloading file [/tmp/bl32.bin]...\n"
            "O.K.\n"
            "Downloading file [/tmp/xipImage]...\n"
            "Writing target memory failed.\n"
        )
        results = _parse_loadbin_output(stdout)
        assert len(results) == 2
        assert results[0]["success"] is True
        assert results[1]["success"] is False

    def test_empty(self):
        assert _parse_loadbin_output("") == []

    def test_noise_in_output(self):
        stdout = (
            "SEGGER J-Link Commander V8.70\n"
            "Connecting to J-Link via USB...O.K.\n"
            "ResetTarget() start\n"
            "****** Error: Failed to halt CPU.\n"
            "Downloading file [/tmp/test.bin]...\n"
            "O.K.\n"
        )
        results = _parse_loadbin_output(stdout)
        assert len(results) == 1
        assert results[0] == {"file": "test.bin", "success": True}

    def test_halt_error_not_treated_as_failure(self):
        """'Failed to halt CPU' between download and O.K. should not fail."""
        stdout = (
            "Downloading file [/tmp/bl32.bin]...\n"
            "O.K.\n"
            "****** Error: Failed to halt CPU.\n"
            "Downloading file [/tmp/xipImage]...\n"
            "****** Error: Failed to halt CPU.\n"
            "O.K.\n"
        )
        results = _parse_loadbin_output(stdout)
        assert len(results) == 2
        assert all(r["success"] for r in results)

    def test_unsupported_format(self):
        """JLinkExe rejects non-.bin extensions with 'unsupported format'."""
        stdout = (
            "Downloading file [/tmp/appkit-e7.dtb]...\n"
            "File is of unknown / unsupported format.\n"
        )
        results = _parse_loadbin_output(stdout)
        assert len(results) == 1
        assert results[0]["success"] is False
        assert "unsupported" in results[0]["error"].lower()
        assert results[0]["file"] == "appkit-e7.dtb"

    def test_mixed_with_unsupported(self):
        """One .bin succeeds, one non-.bin gets unsupported format."""
        stdout = (
            "Downloading file [/tmp/bl32.bin]...\n"
            "O.K.\n"
            "Downloading file [/tmp/appkit-e7.dtb]...\n"
            "File is of unknown / unsupported format.\n"
        )
        results = _parse_loadbin_output(stdout)
        assert len(results) == 2
        assert results[0]["success"] is True
        assert results[1]["success"] is False


class TestMramLayout:
    def test_all_components_defined(self):
        assert "tfa" in MRAM_LAYOUT
        assert "dtb" in MRAM_LAYOUT
        assert "kernel" in MRAM_LAYOUT
        assert "rootfs" in MRAM_LAYOUT

    def test_addresses(self):
        assert MRAM_LAYOUT["tfa"]["addr"] == 0x80002000
        assert MRAM_LAYOUT["dtb"]["addr"] == 0x80010000
        assert MRAM_LAYOUT["kernel"]["addr"] == 0x80020000
        assert MRAM_LAYOUT["rootfs"]["addr"] == 0x80300000

    def test_filenames(self):
        assert MRAM_LAYOUT["tfa"]["file"] == "bl32.bin"
        assert MRAM_LAYOUT["dtb"]["file"] == "appkit-e7.dtb"
        assert MRAM_LAYOUT["kernel"]["file"] == "xipImage"
        assert MRAM_LAYOUT["rootfs"]["file"] == "cramfs-xip.img"

    def test_addresses_non_overlapping(self):
        """Components should not overlap in MRAM."""
        addrs = sorted(MRAM_LAYOUT[k]["addr"] for k in MRAM_LAYOUT)
        for i in range(len(addrs) - 1):
            assert addrs[i] < addrs[i + 1]


class TestAtocKeyMap:
    def test_keys(self):
        assert ATOC_KEY_MAP == {
            "TFA": "tfa", "DTB": "dtb",
            "KERNEL": "kernel", "ROOTFS": "rootfs",
        }


class TestCheckSetup:
    def test_returns_dict(self):
        result = check_setup()
        assert isinstance(result, dict)
        assert "ready" in result
        assert "issues" in result
        assert isinstance(result["issues"], list)

    def test_device_dir_path(self):
        result = check_setup()
        assert result["device_dir"] == JLINK_DEVICES_DIR


class TestFlashImages:
    def test_unknown_component(self):
        from alif_flash.jlink import flash_images
        result = flash_images("/nonexistent", components=["bogus"])
        assert result["success"] is False
        assert "Unknown component" in result["message"]

    def test_missing_files(self):
        from alif_flash.jlink import flash_images
        with tempfile.TemporaryDirectory() as d:
            result = flash_images(d, components=["tfa"])
            assert result["success"] is False
            assert "not found" in result["message"]
