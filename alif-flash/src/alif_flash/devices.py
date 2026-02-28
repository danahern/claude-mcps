"""Device registry for Alif Ensemble boards.

Maps board names to hardware-specific configuration: J-Link device names,
MRAM layouts, baud rates, and tool paths.
"""

DEFAULT_DEVICE = "alif-e7"

DEVICES = {
    "alif-e7": {
        "jlink_device": "AE722F80F55D5_M55_HP",
        "jlink_device_reset": "Cortex-A32",
        "isp_baud": 57600,
        "part_number": "AE722F80F55D5",
        "mram_layout": {
            "tfa":    {"file": "bl32.bin",        "addr": 0x80002000},
            "dtb":    {"file": "appkit-e7.dtb",   "addr": 0x80010000},
            "kernel": {"file": "xipImage",        "addr": 0x80020000},
            "rootfs": {"file": "cramfs-xip.img",  "addr": 0x80300000},
        },
        "atoc_key_map": {"TFA": "tfa", "DTB": "dtb", "KERNEL": "kernel", "ROOTFS": "rootfs"},
        "system_mram_base": 0x80580000,
        "jlink_script": "AlifE7.JLinkScript",
        # SETOOLS global-cfg.db content — written before gen_toc to prevent
        # cross-contamination between boards (E8 clocks on E7 = garbled UART).
        "global_cfg": {
            "DEVICE": {
                "Revision": "B4",
                "Part#": "E7 (AE722F80F55D5LS) - 5.5 MRAM / 13.5 SRAM",
            },
            "MRAM-BURNER": {
                "Interface": "isp",
                "Jtag-adapter": "J-Link",
            },
        },
    },
    "alif-e8": {
        "jlink_device": "AE822FA0E5597_M55_HP",  # derived from SE firmware package names
        "jlink_device_reset": "Cortex-A32",
        "isp_baud": 57600,
        "part_number": "AE822FA0E5597",
        "mram_layout": {
            "tfa":    {"file": "bl32.bin",          "addr": 0x80002000},
            "dtb":    {"file": "devkit-e8.dtb",     "addr": 0x80010000},
            "kernel": {"file": "xipImage",          "addr": 0x80020000},
            "rootfs": {"file": "cramfs-xip.img",    "addr": 0x80380000},
        },
        "atoc_key_map": {"TFA": "tfa", "DTB": "dtb", "KERNEL": "kernel", "ROOTFS": "rootfs"},
        "system_mram_base": 0x80580000,
        "jlink_script": "AlifE7.JLinkScript",  # same TF-A platform (devkit_e7)
        "global_cfg": {
            "DEVICE": {
                "Revision": "A0",
                "Part#": "E8 (AE822FA0E5597LS0) - 5.5 MRAM / 9.75 SRAM",
            },
            "MRAM-BURNER": {
                "Interface": "isp",
                "Jtag-adapter": "J-Link",
            },
        },
    },
}


def get_config(device: str | None = None) -> dict:
    """Get device configuration by name. Defaults to alif-e7."""
    name = device or DEFAULT_DEVICE
    if name not in DEVICES:
        raise ValueError(f"Unknown device '{name}'. Available: {', '.join(sorted(DEVICES))}")
    return DEVICES[name]


def list_devices() -> list[str]:
    """List available device names."""
    return sorted(DEVICES.keys())
