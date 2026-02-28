"""Tests for device registry."""

import pytest

from alif_flash.devices import DEVICES, DEFAULT_DEVICE, get_config, list_devices


class TestDeviceRegistry:
    def test_default_device_exists(self):
        assert DEFAULT_DEVICE in DEVICES

    def test_e7_config(self):
        cfg = DEVICES["alif-e7"]
        assert cfg["jlink_device"] == "AE722F80F55D5_M55_HP"
        assert cfg["isp_baud"] == 57600
        assert cfg["mram_layout"]["rootfs"]["addr"] == 0x80300000

    def test_e8_config(self):
        cfg = DEVICES["alif-e8"]
        assert cfg["jlink_device"] == "AE822FA0E5597_M55_HP"
        assert cfg["mram_layout"]["rootfs"]["addr"] == 0x80380000
        assert cfg["jlink_device_reset"] == "Cortex-A32"

    def test_both_devices_have_required_keys(self):
        required = [
            "jlink_device", "jlink_device_reset", "isp_baud", "part_number",
            "mram_layout", "atoc_key_map", "system_mram_base", "jlink_script",
            "global_cfg",
        ]
        for name, cfg in DEVICES.items():
            for key in required:
                assert key in cfg, f"Device '{name}' missing key '{key}'"

    def test_mram_layouts_have_required_components(self):
        for name, cfg in DEVICES.items():
            layout = cfg["mram_layout"]
            for comp in ["tfa", "dtb", "kernel", "rootfs"]:
                assert comp in layout, f"Device '{name}' missing component '{comp}'"
                assert "file" in layout[comp]
                assert "addr" in layout[comp]

    def test_e7_e8_rootfs_differ(self):
        """E7 and E8 have different rootfs addresses."""
        e7 = DEVICES["alif-e7"]["mram_layout"]["rootfs"]["addr"]
        e8 = DEVICES["alif-e8"]["mram_layout"]["rootfs"]["addr"]
        assert e7 != e8
        assert e7 == 0x80300000
        assert e8 == 0x80380000

    def test_global_cfg_matches_device(self):
        """global_cfg Part# must contain the device's part_number."""
        for name, cfg in DEVICES.items():
            part = cfg["global_cfg"]["DEVICE"]["Part#"]
            assert cfg["part_number"] in part, (
                f"Device '{name}': part_number '{cfg['part_number']}' "
                f"not found in global_cfg Part# '{part}'"
            )

    def test_global_cfg_has_required_keys(self):
        """global_cfg must have DEVICE and MRAM-BURNER sections."""
        for name, cfg in DEVICES.items():
            gcfg = cfg["global_cfg"]
            assert "DEVICE" in gcfg, f"Device '{name}' global_cfg missing DEVICE"
            assert "MRAM-BURNER" in gcfg, f"Device '{name}' global_cfg missing MRAM-BURNER"
            assert "Part#" in gcfg["DEVICE"]
            assert "Revision" in gcfg["DEVICE"]

    def test_e7_e8_global_cfg_differ(self):
        """E7 and E8 must have different global_cfg to prevent cross-contamination."""
        e7_part = DEVICES["alif-e7"]["global_cfg"]["DEVICE"]["Part#"]
        e8_part = DEVICES["alif-e8"]["global_cfg"]["DEVICE"]["Part#"]
        assert e7_part != e8_part
        assert "AE722F80F55D5" in e7_part
        assert "AE822FA0E5597" in e8_part


class TestGetConfig:
    def test_default(self):
        cfg = get_config()
        assert cfg == DEVICES[DEFAULT_DEVICE]

    def test_explicit_e7(self):
        cfg = get_config("alif-e7")
        assert cfg["jlink_device"] == "AE722F80F55D5_M55_HP"

    def test_explicit_e8(self):
        cfg = get_config("alif-e8")
        assert cfg["mram_layout"]["rootfs"]["addr"] == 0x80380000

    def test_none_returns_default(self):
        cfg = get_config(None)
        assert cfg == DEVICES[DEFAULT_DEVICE]

    def test_unknown_device_raises(self):
        with pytest.raises(ValueError, match="Unknown device"):
            get_config("bogus-board")

    def test_unknown_device_lists_available(self):
        with pytest.raises(ValueError, match="alif-e7"):
            get_config("bogus-board")


class TestListDevices:
    def test_returns_list(self):
        result = list_devices()
        assert isinstance(result, list)
        assert "alif-e7" in result
        assert "alif-e8" in result

    def test_sorted(self):
        result = list_devices()
        assert result == sorted(result)
