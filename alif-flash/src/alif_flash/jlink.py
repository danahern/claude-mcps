"""J-Link flash programming for Alif E7 (MRAM + OSPI).

Direct loadbin writes via JLinkExe. MRAM writes are direct (~44 KB/s).
OSPI writes go through the Ensemble_IS25WX256.FLM flash loader (slower
due to erase cycles). Both use the same loadbin command — J-Link routes
writes to OSPI addresses (>= 0xC0000000) through the flash algorithm
automatically via the FlashBankInfo in Devices.xml.

Requires one-time setup: Devices.xml + AlifE7.JLinkScript installed to
~/Library/Application Support/SEGGER/JLinkDevices/AlifSemi/.
For OSPI: Ensemble_IS25WX256.FLM must also be present (from Segger's
Alif device pack).
"""

import logging
import os
import re
import shutil
import subprocess
import tempfile
import time

from . import devices

logger = logging.getLogger(__name__)

JLINK_EXE = "/usr/local/bin/JLinkExe"

# Backward-compatible module-level defaults (alif-e7)
_DEFAULT_CFG = devices.get_config()
DEVICE = _DEFAULT_CFG["jlink_device"]
DEVICE_RESET = _DEFAULT_CFG["jlink_device_reset"]

INTERFACE = "SWD"
SPEED = "auto"

# SEGGER user device directory (macOS)
JLINK_DEVICES_DIR = os.path.expanduser(
    "~/Library/Application Support/SEGGER/JLinkDevices/AlifSemi"
)
JLINK_SCRIPT_FILE = os.path.join(JLINK_DEVICES_DIR, _DEFAULT_CFG["jlink_script"])

# MRAM layout — E7 defaults (use devices.get_config(device) for other boards)
MRAM_LAYOUT = dict(_DEFAULT_CFG["mram_layout"])

# Maps ATOC JSON keys to component names
ATOC_KEY_MAP = dict(_DEFAULT_CFG["atoc_key_map"])


def _jlink_data_dir() -> str:
    """Path to the jlink/ data directory shipped with this package."""
    return os.path.join(os.path.dirname(__file__), "..", "..", "jlink")


OSPI_FLM_NAME = "Ensemble_IS25WX256.FLM"

# Addresses at or above this threshold are routed through the flash loader
OSPI_ADDR_THRESHOLD = 0xA0000000

# OSPI flash erase sector size (IS25WX256/512 use 64KB sectors)
OSPI_ERASE_SECTOR = 0x10000


def check_setup(device: str | None = None) -> dict:
    """Check if JLinkExe and device definition are installed."""
    cfg = devices.get_config(device)
    issues = []
    warnings = []

    if not os.path.exists(JLINK_EXE):
        issues.append(f"JLinkExe not found at {JLINK_EXE}")

    xml_path = os.path.join(JLINK_DEVICES_DIR, "Devices.xml")
    script_path = os.path.join(JLINK_DEVICES_DIR, cfg["jlink_script"])
    flm_path = os.path.join(JLINK_DEVICES_DIR, OSPI_FLM_NAME)

    if not os.path.exists(xml_path):
        issues.append(f"Devices.xml not found at {xml_path}")
    if not os.path.exists(script_path):
        issues.append(f"AlifE7.JLinkScript not found at {script_path}")

    if not os.path.exists(flm_path):
        warnings.append(
            f"OSPI flash loader ({OSPI_FLM_NAME}) not found at {flm_path}. "
            "MRAM programming works without it. For OSPI support, install "
            "from Segger's Alif Ensemble device pack."
        )

    result = {
        "ready": len(issues) == 0,
        "jlink_exe": JLINK_EXE,
        "device_dir": JLINK_DEVICES_DIR,
        "issues": issues,
    }
    if warnings:
        result["warnings"] = warnings
    return result


def install_device_def(device: str | None = None) -> dict:
    """Install Devices.xml + JLinkScript to SEGGER user directory."""
    cfg = devices.get_config(device)
    src_dir = _jlink_data_dir()
    os.makedirs(JLINK_DEVICES_DIR, exist_ok=True)

    copied = []
    for name in ("Devices.xml", cfg["jlink_script"]):
        src = os.path.join(src_dir, name)
        dst = os.path.join(JLINK_DEVICES_DIR, name)
        if not os.path.exists(src):
            return {"success": False, "message": f"Source file not found: {src}"}
        shutil.copy2(src, dst)
        copied.append(name)

    result = {"success": True, "installed": copied, "dest": JLINK_DEVICES_DIR}

    flm_path = os.path.join(JLINK_DEVICES_DIR, OSPI_FLM_NAME)
    if not os.path.exists(flm_path):
        result["note"] = (
            f"OSPI flash loader ({OSPI_FLM_NAME}) not found. "
            "MRAM programming works without it. For OSPI support, install "
            "from Segger's Alif Ensemble device pack."
        )

    return result


def _run_jlink(script_content: str, device: str | None = None, timeout: int = 120) -> dict:
    """Run JLinkExe with a command script. Returns parsed output."""
    cfg = devices.get_config(device)
    jlink_device = cfg["jlink_device"]
    jlink_script_path = os.path.join(JLINK_DEVICES_DIR, cfg["jlink_script"])

    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".jlink", delete=False
    ) as f:
        f.write(script_content)
        script_path = f.name

    try:
        cmd = [JLINK_EXE, "-device", jlink_device, "-if", INTERFACE,
               "-speed", str(SPEED), "-autoconnect", "1",
               "-NoGui", "1",
               "-CommandFile", script_path]
        # JLink V9.20 doesn't resolve JLinkScriptFile from Devices.xml
        # correctly — pass it explicitly on the command line.
        if os.path.exists(jlink_script_path):
            cmd.extend(["-JLinkScriptFile", jlink_script_path])
        result = subprocess.run(
            cmd, capture_output=True, text=True, timeout=timeout,
        )
        stdout = result.stdout

        # Check for real connection failures (not "Failed to halt CPU" which is expected)
        if "Could not connect" in stdout or "No J-Link found" in stdout:
            return {"success": False, "message": "J-Link not connected",
                    "stdout": stdout[-1000:]}
        if "Writing target memory failed" in stdout:
            return {"success": False, "message": "MRAM write failed",
                    "stdout": stdout[-1000:]}
        if "unsupported format" in stdout.lower():
            return {"success": False, "message": "File format rejected by JLinkExe (extension issue)",
                    "stdout": stdout[-1000:]}

        # "Failed to halt CPU" is normal — writes succeed anyway
        ok = True

        return {"success": ok, "returncode": result.returncode,
                "stdout": stdout, "stderr": result.stderr}
    except subprocess.TimeoutExpired:
        return {"success": False, "message": f"JLinkExe timed out after {timeout}s"}
    except FileNotFoundError:
        return {"success": False, "message": f"JLinkExe not found at {JLINK_EXE}"}
    finally:
        os.unlink(script_path)


def _parse_loadbin_output(stdout: str) -> list[dict]:
    """Parse JLinkExe output to extract per-file results."""
    results = []
    # Match lines like: "Downloading file [/path/to/file.bin]..."
    # Followed by "O.K.", "Writing target memory failed.", or "unsupported format"
    lines = stdout.split("\n")
    current_file = None
    for line in lines:
        m = re.search(r"Downloading file \[(.+?)\]", line)
        if m:
            current_file = os.path.basename(m.group(1))
        elif current_file and "O.K." in line:
            results.append({"file": current_file, "success": True})
            current_file = None
        elif current_file and "Writing target memory failed" in line:
            results.append({"file": current_file, "success": False, "error": line.strip()})
            current_file = None
        elif current_file and "unsupported format" in line.lower():
            results.append({"file": current_file, "success": False, "error": line.strip()})
            current_file = None

    return results


def flash_images(image_dir: str, components: list[str] | None = None,
                 verify: bool = False, erase: bool = False,
                 device: str | None = None, layout: dict | None = None) -> dict:
    """Flash Linux images to MRAM via JLinkExe loadbin.

    Args:
        image_dir: Directory containing the image files.
        components: List of components to flash (tfa, dtb, kernel, rootfs).
                    Defaults to all.
        verify: Run verifybin after each loadbin.
        erase: Erase OSPI flash region before programming. Clears stale data
               that MTD partition parsers (RedBoot/AFS) might misinterpret.
        device: Target device name (default: alif-e7).
        layout: Custom MRAM layout dict. If None, uses device's default layout.
    """
    if layout is None:
        cfg = devices.get_config(device)
        layout = cfg["mram_layout"]

    # Auto-install device definition if needed
    setup = check_setup(device=device)
    if not setup["ready"]:
        if any("JLinkExe" in i for i in setup["issues"]):
            return {"success": False, "message": "JLinkExe not installed",
                    "issues": setup["issues"]}
        logger.info("Device definition not installed, installing...")
        install_result = install_device_def(device=device)
        if not install_result["success"]:
            return {"success": False, "message": "Failed to install device definition",
                    "detail": install_result}

    if components is None:
        components = list(layout.keys())

    # Validate files
    files_to_flash = []
    for comp in components:
        if comp not in layout:
            return {"success": False,
                    "message": f"Unknown component '{comp}'. Use: {', '.join(layout)}"}
        info = layout[comp]
        path = os.path.join(image_dir, info["file"])
        if not os.path.exists(path):
            return {"success": False, "message": f"File not found: {path}"}
        files_to_flash.append((comp, path, info["addr"]))

    # JLinkExe loadbin rejects files with non-.bin extensions (.dtb, .img, etc.)
    # Copy such files to a temp directory with .bin extension.
    tmp_dir = None
    load_files = []  # (comp, load_path, orig_path, addr) — load_path may differ from orig
    for comp, path, addr in files_to_flash:
        if path.endswith(".bin"):
            load_files.append((comp, path, path, addr))
        else:
            if tmp_dir is None:
                tmp_dir = tempfile.mkdtemp(prefix="jlink_")
            base = os.path.splitext(os.path.basename(path))[0] or os.path.basename(path)
            tmp_path = os.path.join(tmp_dir, base + ".bin")
            shutil.copy2(path, tmp_path)
            load_files.append((comp, tmp_path, path, addr))

    try:
        # Build JLink command script
        lines = []

        # Pre-erase OSPI region to clear stale partition table data
        if erase:
            ospi_files = [(comp, lp, op, addr) for comp, lp, op, addr in load_files
                          if addr >= OSPI_ADDR_THRESHOLD]
            if ospi_files:
                # Erase from lowest OSPI addr to end of highest file, rounded up to sector boundary
                min_addr = min(addr for _, _, _, addr in ospi_files)
                max_end = max(addr + os.path.getsize(op) for _, _, op, addr in ospi_files)
                erase_end = ((max_end + OSPI_ERASE_SECTOR - 1) // OSPI_ERASE_SECTOR) * OSPI_ERASE_SECTOR
                logger.info("Pre-erasing OSPI region 0x%08X - 0x%08X (%d KB)",
                            min_addr, erase_end, (erase_end - min_addr) // 1024)
                lines.append(f"erase 0x{min_addr:08X} 0x{erase_end:08X}")
                lines.append("")

        for comp, load_path, orig_path, addr in load_files:
            lines.append(f"loadbin {load_path} 0x{addr:08X}")
        if verify:
            lines.append("")
            for comp, load_path, orig_path, addr in load_files:
                lines.append(f"verifybin {load_path} 0x{addr:08X}")
        lines.append("exit")
        script = "\n".join(lines) + "\n"

        # Calculate total size
        total_bytes = sum(os.path.getsize(p) for _, _, p, _ in load_files)

        logger.info("Flashing %d components (%d KB) via J-Link...",
                    len(load_files), total_bytes // 1024)
        for comp, load_path, orig_path, addr in load_files:
            size = os.path.getsize(orig_path)
            logger.info("  %-10s %s @ 0x%08X (%d bytes)", comp, os.path.basename(orig_path), addr, size)

        # OSPI flash programming is ~7 KB/s (erase cycles) — scale timeout to data size
        has_ospi = any(addr >= OSPI_ADDR_THRESHOLD for _, _, _, addr in load_files)
        if has_ospi:
            # ~7 KB/s + overhead for erase/verify. 2x safety margin.
            timeout = max(600, (total_bytes // 3500) * 2)
        else:
            timeout = 300

        t0 = time.time()
        result = _run_jlink(script, device=device, timeout=timeout)
        elapsed = time.time() - t0

        if not result["success"]:
            return {
                "success": False,
                "message": result.get("message", "JLinkExe failed"),
                "stdout": result.get("stdout", "")[-1000:],
                "elapsed_seconds": round(elapsed, 1),
            }

        # Parse per-file results — map temp .bin names back to originals
        file_results = _parse_loadbin_output(result["stdout"])
        tmp_to_orig = {}
        for comp, load_path, orig_path, addr in load_files:
            tmp_to_orig[os.path.basename(load_path)] = os.path.basename(orig_path)
        for r in file_results:
            r["file"] = tmp_to_orig.get(r["file"], r["file"])

        all_ok = all(r["success"] for r in file_results) if file_results else False

        # Check verify results
        verified = "Verify successful" in result.get("stdout", "") if verify else None

        return {
            "success": all_ok,
            "method": "jlink_loadbin",
            "total_bytes": total_bytes,
            "elapsed_seconds": round(elapsed, 1),
            "bytes_per_second": round(total_bytes / elapsed) if elapsed > 0 else 0,
            "components": len(files_to_flash),
            "files": file_results,
            "verified": verified,
            "message": (
                f"{'All' if all_ok else 'Some'} images written via J-Link in {elapsed:.1f}s. "
                "Power cycle (unplug/replug PRG_USB) for A32 to boot."
            ),
        }
    finally:
        if tmp_dir is not None:
            shutil.rmtree(tmp_dir, ignore_errors=True)


def flash_from_config(config_path: str, verify: bool = False,
                      erase: bool = False, device: str | None = None) -> dict:
    """Flash images defined in an ATOC JSON config via J-Link.

    Reads the same JSON format as the SE-UART flash tool, extracting
    file names and MRAM addresses from each entry. Handles ANY config
    key with mramAddress + binary fields, not just known keys.
    """
    import json

    cfg = devices.get_config(device)
    atoc_key_map = cfg["atoc_key_map"]

    with open(config_path) as f:
        config = json.load(f)

    build_dir = os.path.normpath(os.path.join(os.path.dirname(config_path), ".."))
    images_dir = os.path.join(build_dir, "images")

    # Build layout from config — process ALL keys generically
    custom_layout = {}
    for key, entry in config.items():
        if key == "DEVICE" or not isinstance(entry, dict):
            continue
        if entry.get("disabled", False):
            continue
        binary = entry.get("binary")
        addr_str = entry.get("address") or entry.get("mramAddress") or entry.get("ospiAddress")
        if binary and addr_str:
            addr = int(addr_str, 16)
            comp = atoc_key_map.get(key, key.lower())
            custom_layout[comp] = {"file": binary, "addr": addr}

    if not custom_layout:
        return {"success": False, "message": "No images found in config"}

    components = list(custom_layout.keys())
    return flash_images(images_dir, components, verify, erase, layout=custom_layout)
