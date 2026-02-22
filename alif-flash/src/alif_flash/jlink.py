"""J-Link MRAM programming for Alif E7.

Direct loadbin writes via JLinkExe at ~44 KB/s (9x faster than SE-UART).
Requires one-time setup: Devices.xml + AlifE7.JLinkScript installed to
~/Library/Application Support/SEGGER/JLinkDevices/AlifSemi/.
"""

import logging
import os
import re
import shutil
import subprocess
import tempfile
import time

logger = logging.getLogger(__name__)

JLINK_EXE = "/usr/local/bin/JLinkExe"
DEVICE = "AE722F80F55D5_M55_HP"
INTERFACE = "SWD"
SPEED = 4000

# SEGGER user device directory (macOS)
JLINK_DEVICES_DIR = os.path.expanduser(
    "~/Library/Application Support/SEGGER/JLinkDevices/AlifSemi"
)

# MRAM layout (appkit-e7.conf, devkit-ex-b0 branch)
MRAM_LAYOUT = {
    "tfa":    {"file": "bl32.bin",        "addr": 0x80002000},
    "dtb":    {"file": "appkit-e7.dtb",   "addr": 0x80010000},
    "kernel": {"file": "xipImage",        "addr": 0x80020000},
    "rootfs": {"file": "cramfs-xip.img",  "addr": 0x80300000},
}

# Maps ATOC JSON keys to component names
ATOC_KEY_MAP = {"TFA": "tfa", "DTB": "dtb", "KERNEL": "kernel", "ROOTFS": "rootfs"}


def _jlink_data_dir() -> str:
    """Path to the jlink/ data directory shipped with this package."""
    return os.path.join(os.path.dirname(__file__), "..", "..", "jlink")


def check_setup() -> dict:
    """Check if JLinkExe and device definition are installed."""
    issues = []

    if not os.path.exists(JLINK_EXE):
        issues.append(f"JLinkExe not found at {JLINK_EXE}")

    xml_path = os.path.join(JLINK_DEVICES_DIR, "Devices.xml")
    script_path = os.path.join(JLINK_DEVICES_DIR, "AlifE7.JLinkScript")

    if not os.path.exists(xml_path):
        issues.append(f"Devices.xml not found at {xml_path}")
    if not os.path.exists(script_path):
        issues.append(f"AlifE7.JLinkScript not found at {script_path}")

    return {
        "ready": len(issues) == 0,
        "jlink_exe": JLINK_EXE,
        "device_dir": JLINK_DEVICES_DIR,
        "issues": issues,
    }


def install_device_def() -> dict:
    """Install Devices.xml + AlifE7.JLinkScript to SEGGER user directory."""
    src_dir = _jlink_data_dir()
    os.makedirs(JLINK_DEVICES_DIR, exist_ok=True)

    copied = []
    for name in ("Devices.xml", "AlifE7.JLinkScript"):
        src = os.path.join(src_dir, name)
        dst = os.path.join(JLINK_DEVICES_DIR, name)
        if not os.path.exists(src):
            return {"success": False, "message": f"Source file not found: {src}"}
        shutil.copy2(src, dst)
        copied.append(name)

    return {"success": True, "installed": copied, "dest": JLINK_DEVICES_DIR}


def _run_jlink(script_content: str, timeout: int = 120) -> dict:
    """Run JLinkExe with a command script. Returns parsed output."""
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".jlink", delete=False
    ) as f:
        f.write(script_content)
        script_path = f.name

    try:
        result = subprocess.run(
            [JLINK_EXE, "-device", DEVICE, "-if", INTERFACE,
             "-speed", str(SPEED), "-autoconnect", "1",
             "-CommandFile", script_path],
            capture_output=True, text=True, timeout=timeout,
        )
        stdout = result.stdout
        ok = result.returncode == 0 or "Verify successful" in stdout

        # Check for connection failures
        if "Could not connect" in stdout or "No J-Link found" in stdout:
            return {"success": False, "message": "J-Link not connected",
                    "stdout": stdout[-1000:]}

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
    # Followed by "O.K." or "Writing target memory failed."
    lines = stdout.split("\n")
    current_file = None
    for line in lines:
        m = re.search(r"Downloading file \[(.+?)\]", line)
        if m:
            current_file = os.path.basename(m.group(1))
        elif current_file and "O.K." in line:
            results.append({"file": current_file, "success": True})
            current_file = None
        elif current_file and "failed" in line.lower():
            results.append({"file": current_file, "success": False, "error": line.strip()})
            current_file = None

    return results


def flash_images(image_dir: str, components: list[str] | None = None,
                 verify: bool = False) -> dict:
    """Flash Linux images to MRAM via JLinkExe loadbin.

    Args:
        image_dir: Directory containing the image files.
        components: List of components to flash (tfa, dtb, kernel, rootfs).
                    Defaults to all.
        verify: Run verifybin after each loadbin.
    """
    # Auto-install device definition if needed
    setup = check_setup()
    if not setup["ready"]:
        if any("JLinkExe" in i for i in setup["issues"]):
            return {"success": False, "message": "JLinkExe not installed",
                    "issues": setup["issues"]}
        logger.info("Device definition not installed, installing...")
        install_result = install_device_def()
        if not install_result["success"]:
            return {"success": False, "message": "Failed to install device definition",
                    "detail": install_result}

    if components is None:
        components = list(MRAM_LAYOUT.keys())

    # Validate files
    files_to_flash = []
    for comp in components:
        if comp not in MRAM_LAYOUT:
            return {"success": False,
                    "message": f"Unknown component '{comp}'. Use: {', '.join(MRAM_LAYOUT)}"}
        info = MRAM_LAYOUT[comp]
        path = os.path.join(image_dir, info["file"])
        if not os.path.exists(path):
            return {"success": False, "message": f"File not found: {path}"}
        files_to_flash.append((comp, path, info["addr"]))

    # Build JLink command script
    lines = []
    for comp, path, addr in files_to_flash:
        lines.append(f"loadbin {path} 0x{addr:08X}")
    if verify:
        lines.append("")
        for comp, path, addr in files_to_flash:
            lines.append(f"verifybin {path} 0x{addr:08X}")
    lines.append("exit")
    script = "\n".join(lines) + "\n"

    # Calculate total size
    total_bytes = sum(os.path.getsize(p) for _, p, _ in files_to_flash)

    logger.info("Flashing %d components (%d KB) via J-Link...",
                len(files_to_flash), total_bytes // 1024)
    for comp, path, addr in files_to_flash:
        size = os.path.getsize(path)
        logger.info("  %-10s %s @ 0x%08X (%d bytes)", comp, os.path.basename(path), addr, size)

    t0 = time.time()
    result = _run_jlink(script, timeout=300)
    elapsed = time.time() - t0

    if not result["success"]:
        return {
            "success": False,
            "message": result.get("message", "JLinkExe failed"),
            "stdout": result.get("stdout", "")[-1000:],
            "elapsed_seconds": round(elapsed, 1),
        }

    # Parse per-file results
    file_results = _parse_loadbin_output(result["stdout"])
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


def flash_from_config(config_path: str, verify: bool = False) -> dict:
    """Flash images defined in an ATOC JSON config via J-Link.

    Reads the same JSON format as the SE-UART flash tool, extracting
    file names and MRAM addresses from each entry.
    """
    import json

    with open(config_path) as f:
        config = json.load(f)

    build_dir = os.path.normpath(os.path.join(os.path.dirname(config_path), ".."))
    images_dir = os.path.join(build_dir, "images")

    # Build component list from config
    components = []
    custom_layout = {}
    for key, comp in ATOC_KEY_MAP.items():
        entry = config.get(key, {})
        if entry.get("disabled", False):
            continue
        binary = entry.get("binary")
        addr_str = entry.get("mramAddress")
        if binary and addr_str:
            addr = int(addr_str, 16)
            custom_layout[comp] = {"file": binary, "addr": addr}
            components.append(comp)

    if not components:
        return {"success": False, "message": "No images found in config"}

    # Override MRAM_LAYOUT temporarily with config values
    saved = {}
    for comp, info in custom_layout.items():
        saved[comp] = MRAM_LAYOUT.get(comp)
        MRAM_LAYOUT[comp] = info

    try:
        return flash_images(images_dir, components, verify)
    finally:
        for comp, orig in saved.items():
            if orig is not None:
                MRAM_LAYOUT[comp] = orig
            else:
                del MRAM_LAYOUT[comp]
