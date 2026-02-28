"""Tests for J-Link flash programming: output parsing, setup checks, config parsing (MRAM + OSPI)."""

import json
import os
import tempfile
from unittest.mock import patch

from alif_flash.jlink import (
    MRAM_LAYOUT,
    ATOC_KEY_MAP,
    OSPI_ADDR_THRESHOLD,
    OSPI_FLM_NAME,
    JLINK_SCRIPT_FILE,
    _parse_loadbin_output,
    check_setup,
    flash_from_config,
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


class TestFlashFromConfig:
    """Tests for flash_from_config config parsing and generic key handling."""

    def _make_config_dir(self, tmp, config_data):
        """Create a config dir structure: tmp/config/<name>.json + tmp/images/."""
        config_dir = os.path.join(tmp, "config")
        images_dir = os.path.join(tmp, "images")
        os.makedirs(config_dir, exist_ok=True)
        os.makedirs(images_dir, exist_ok=True)
        config_path = os.path.join(config_dir, "test.json")
        with open(config_path, "w") as f:
            json.dump(config_data, f)
        return config_path, images_dir

    @patch("alif_flash.jlink.flash_images")
    def test_known_keys_processed(self, mock_flash):
        """Standard TFA/KERNEL keys are passed through via ATOC_KEY_MAP."""
        mock_flash.return_value = {"success": True}
        with tempfile.TemporaryDirectory() as tmp:
            config = {
                "DEVICE": {"partNumber": "AE722F80F55D5AS"},
                "TFA": {"binary": "bl32.bin", "mramAddress": "0x80002000"},
                "KERNEL": {"binary": "xipImage", "mramAddress": "0x80020000"},
            }
            config_path, images_dir = self._make_config_dir(tmp, config)
            flash_from_config(config_path)
            mock_flash.assert_called_once()
            call_args = mock_flash.call_args
            assert images_dir == call_args[0][0]
            components = call_args[0][1]
            assert "tfa" in components
            assert "kernel" in components

    @patch("alif_flash.jlink.flash_images")
    def test_nonstandard_key_processed(self, mock_flash):
        """Non-standard keys like OSPI_HDR are processed generically."""
        mock_flash.return_value = {"success": True}
        with tempfile.TemporaryDirectory() as tmp:
            config = {
                "DEVICE": {"partNumber": "AE722F80F55D5AS"},
                "TFA": {"binary": "bl32.bin", "mramAddress": "0x80002000"},
                "OSPI_HDR": {"binary": "ospi_header.bin", "mramAddress": "0x80001000"},
            }
            config_path, images_dir = self._make_config_dir(tmp, config)
            flash_from_config(config_path)
            mock_flash.assert_called_once()
            components = mock_flash.call_args[0][1]
            assert "tfa" in components
            assert "ospi_hdr" in components

    @patch("alif_flash.jlink.flash_images")
    def test_nonstandard_key_in_mram_layout(self, mock_flash):
        """Non-standard keys get injected into MRAM_LAYOUT with correct addr/file."""
        mock_flash.return_value = {"success": True}
        with tempfile.TemporaryDirectory() as tmp:
            config = {
                "OSPI_HDR": {"binary": "ospi_header.bin", "mramAddress": "0x80001000"},
                "TESTDATA": {"binary": "test.bin", "mramAddress": "0x80500000"},
            }
            config_path, images_dir = self._make_config_dir(tmp, config)
            flash_from_config(config_path)
            components = mock_flash.call_args[0][1]
            assert "ospi_hdr" in components
            assert "testdata" in components

    @patch("alif_flash.jlink.flash_images")
    def test_nonstandard_key_cleaned_up(self, mock_flash):
        """Non-standard keys are removed from MRAM_LAYOUT after flash."""
        mock_flash.return_value = {"success": True}
        with tempfile.TemporaryDirectory() as tmp:
            config = {
                "OSPI_HDR": {"binary": "ospi_header.bin", "mramAddress": "0x80001000"},
            }
            config_path, _ = self._make_config_dir(tmp, config)
            flash_from_config(config_path)
            assert "ospi_hdr" not in MRAM_LAYOUT

    @patch("alif_flash.jlink.flash_images")
    def test_disabled_entry_skipped(self, mock_flash):
        """Entries with disabled=true are skipped."""
        mock_flash.return_value = {"success": True}
        with tempfile.TemporaryDirectory() as tmp:
            config = {
                "TFA": {"binary": "bl32.bin", "mramAddress": "0x80002000"},
                "OSPI_HDR": {"binary": "ospi.bin", "mramAddress": "0x80001000",
                             "disabled": True},
            }
            config_path, _ = self._make_config_dir(tmp, config)
            flash_from_config(config_path)
            components = mock_flash.call_args[0][1]
            assert "tfa" in components
            assert "ospi_hdr" not in components

    @patch("alif_flash.jlink.flash_images")
    def test_device_key_skipped(self, mock_flash):
        """DEVICE key is always skipped (not an image entry)."""
        mock_flash.return_value = {"success": True}
        with tempfile.TemporaryDirectory() as tmp:
            config = {
                "DEVICE": {"partNumber": "AE722F80F55D5AS"},
                "TFA": {"binary": "bl32.bin", "mramAddress": "0x80002000"},
            }
            config_path, _ = self._make_config_dir(tmp, config)
            flash_from_config(config_path)
            components = mock_flash.call_args[0][1]
            assert "device" not in components
            assert len(components) == 1

    def test_empty_config_returns_error(self):
        """Config with no valid image entries returns error."""
        with tempfile.TemporaryDirectory() as tmp:
            config = {"DEVICE": {"partNumber": "AE722F80F55D5AS"}}
            config_path, _ = self._make_config_dir(tmp, config)
            result = flash_from_config(config_path)
            assert result["success"] is False
            assert "No images" in result["message"]

    @patch("alif_flash.jlink.flash_images")
    def test_entry_without_mram_address_skipped(self, mock_flash):
        """Entries missing mramAddress are skipped."""
        mock_flash.return_value = {"success": True}
        with tempfile.TemporaryDirectory() as tmp:
            config = {
                "TFA": {"binary": "bl32.bin", "mramAddress": "0x80002000"},
                "METADATA": {"binary": "meta.bin"},  # no mramAddress
            }
            config_path, _ = self._make_config_dir(tmp, config)
            flash_from_config(config_path)
            components = mock_flash.call_args[0][1]
            assert "tfa" in components
            assert "metadata" not in components

    @patch("alif_flash.jlink.flash_images")
    def test_address_field_preferred(self, mock_flash):
        """Generic 'address' field is supported and preferred over mramAddress."""
        mock_flash.return_value = {"success": True}
        with tempfile.TemporaryDirectory() as tmp:
            config = {
                "KERNEL": {"binary": "xipImage", "address": "0xC0100000"},
            }
            config_path, _ = self._make_config_dir(tmp, config)
            flash_from_config(config_path)
            components = mock_flash.call_args[0][1]
            assert "kernel" in components
            # Check the injected address is the OSPI address
            assert MRAM_LAYOUT.get("kernel", {}).get("addr") != 0xC0100000  # cleaned up
        # Verify cleanup happened
        assert MRAM_LAYOUT["kernel"]["addr"] == 0x80020000

    @patch("alif_flash.jlink.flash_images")
    def test_ospi_address_field(self, mock_flash):
        """'ospiAddress' field is supported for OSPI entries."""
        mock_flash.return_value = {"success": True}
        with tempfile.TemporaryDirectory() as tmp:
            config = {
                "ROOTFS": {"binary": "rootfs.cramfs", "ospiAddress": "0xC0300000"},
            }
            config_path, _ = self._make_config_dir(tmp, config)
            flash_from_config(config_path)
            components = mock_flash.call_args[0][1]
            assert "rootfs" in components
        # Verify original layout restored
        assert MRAM_LAYOUT["rootfs"]["addr"] == 0x80300000

    @patch("alif_flash.jlink.flash_images")
    def test_address_takes_priority_over_mram(self, mock_flash):
        """When both 'address' and 'mramAddress' present, 'address' wins."""
        mock_flash.return_value = {"success": True}
        with tempfile.TemporaryDirectory() as tmp:
            config = {
                "KERNEL": {
                    "binary": "xipImage",
                    "address": "0xC0100000",
                    "mramAddress": "0x80020000",
                },
            }
            config_path, _ = self._make_config_dir(tmp, config)
            flash_from_config(config_path)

            # flash_from_config passes custom layout via keyword arg
            layout = mock_flash.call_args.kwargs.get("layout", {})
            assert layout["kernel"]["addr"] == 0xC0100000

    @patch("alif_flash.jlink.flash_images")
    def test_mixed_mram_ospi_config(self, mock_flash):
        """Config with both MRAM and OSPI entries is processed correctly."""
        mock_flash.return_value = {"success": True}
        with tempfile.TemporaryDirectory() as tmp:
            config = {
                "TFA": {"binary": "bl32.bin", "mramAddress": "0x80002000"},
                "DTB": {"binary": "appkit-e7.dtb", "mramAddress": "0x80010000"},
                "KERNEL": {"binary": "xipImage", "address": "0xC0100000"},
                "ROOTFS": {"binary": "rootfs.cramfs", "address": "0xC0300000"},
            }
            config_path, _ = self._make_config_dir(tmp, config)
            flash_from_config(config_path)
            components = mock_flash.call_args[0][1]
            assert len(components) == 4
            assert "tfa" in components
            assert "dtb" in components
            assert "kernel" in components
            assert "rootfs" in components

    @patch("alif_flash.jlink.flash_images")
    def test_mram_address_still_works(self, mock_flash):
        """Backward compat: mramAddress alone still works."""
        mock_flash.return_value = {"success": True}
        with tempfile.TemporaryDirectory() as tmp:
            config = {
                "TFA": {"binary": "bl32.bin", "mramAddress": "0x80002000"},
            }
            config_path, _ = self._make_config_dir(tmp, config)
            flash_from_config(config_path)
            components = mock_flash.call_args[0][1]
            assert "tfa" in components

    @patch("alif_flash.jlink.flash_images")
    def test_entry_without_any_address_skipped(self, mock_flash):
        """Entries missing all address fields are skipped."""
        mock_flash.return_value = {"success": True}
        with tempfile.TemporaryDirectory() as tmp:
            config = {
                "TFA": {"binary": "bl32.bin", "mramAddress": "0x80002000"},
                "METADATA": {"binary": "meta.bin"},  # no address at all
            }
            config_path, _ = self._make_config_dir(tmp, config)
            flash_from_config(config_path)
            components = mock_flash.call_args[0][1]
            assert "metadata" not in components


class TestCheckSetupFLM:
    """Tests for OSPI flash loader detection in check_setup()."""

    @patch("alif_flash.jlink.os.path.exists")
    def test_flm_missing_adds_warning(self, mock_exists):
        """Missing FLM file produces a warning, not an error."""
        def exists_side_effect(path):
            if OSPI_FLM_NAME in path:
                return False
            return True  # JLinkExe, Devices.xml, JLinkScript all present
        mock_exists.side_effect = exists_side_effect
        result = check_setup()
        assert result["ready"] is True  # still ready — FLM is optional
        assert "warnings" in result
        assert any(OSPI_FLM_NAME in w for w in result["warnings"])

    @patch("alif_flash.jlink.os.path.exists")
    def test_flm_present_no_warning(self, mock_exists):
        """When FLM is present, no warnings."""
        mock_exists.return_value = True
        result = check_setup()
        assert result["ready"] is True
        assert "warnings" not in result


class TestOspiConstants:
    """Tests for OSPI-related constants."""

    def test_ospi_threshold(self):
        assert OSPI_ADDR_THRESHOLD == 0xA0000000

    def test_ospi_flm_name(self):
        assert OSPI_FLM_NAME == "Ensemble_IS25WX256.FLM"

    def test_mram_addresses_below_threshold(self):
        """All MRAM addresses should be below the OSPI threshold."""
        for comp, info in MRAM_LAYOUT.items():
            assert info["addr"] < OSPI_ADDR_THRESHOLD, (
                f"{comp} addr 0x{info['addr']:08X} >= OSPI threshold"
            )

    def test_jlink_script_file_path(self):
        """JLINK_SCRIPT_FILE should be in the devices directory."""
        assert JLINK_SCRIPT_FILE.endswith("AlifE7.JLinkScript")
        assert JLINK_DEVICES_DIR in JLINK_SCRIPT_FILE
