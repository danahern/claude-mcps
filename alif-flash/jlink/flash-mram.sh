#!/bin/bash
# Flash Linux images to Alif E7 MRAM via J-Link.
#
# Uses direct loadbin writes through the M55_HP debug port.
# Requires setup.sh to have been run first (installs JLinkScript
# that prevents reset, which would kill the SE boot sequence).
#
# Usage:
#   flash-mram.sh [options] [image_dir]
#
#   image_dir   Directory containing bl32.bin, appkit-e7.dtb, xipImage, cramfs-xip.img
#               Defaults to ../../firmware/linux/alif-e7/images/
#
# Options:
#   -v, --verify    Verify after programming
#   -c, --component NAME  Flash only one component: tfa, dtb, kernel, rootfs
#   -s, --speed SPEED     SWD speed in kHz (default: 4000)
#   -h, --help      Show this help
#
# Note: Board must be freshly power-cycled (unplug/replug PRG_USB) before
# first use. The SE boot sequence makes the M55_HP core available at AP[3].
# Subsequent writes within the same session work without power cycling.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# MRAM layout (from appkit-e7.conf, devkit-ex-b0 branch)
ADDR_TFA=0x80002000
ADDR_DTB=0x80010000
ADDR_KERNEL=0x80020000
ADDR_ROOTFS=0x80300000

# Defaults
VERIFY=false
COMPONENT=""
SPEED=4000
DEVICE="AE722F80F55D5_M55_HP"
IMAGE_DIR="$WORKSPACE_ROOT/firmware/linux/alif-e7/images"

usage() {
    sed -n '2,/^$/s/^# \?//p' "$0"
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -v|--verify)  VERIFY=true; shift ;;
        -c|--component) COMPONENT="$2"; shift 2 ;;
        -s|--speed)   SPEED="$2"; shift 2 ;;
        -h|--help)    usage ;;
        *)            IMAGE_DIR="$1"; shift ;;
    esac
done

# Map component names to files and addresses
declare -A FILES ADDRS
FILES[tfa]="bl32.bin"
FILES[dtb]="appkit-e7.dtb"
FILES[kernel]="xipImage"
FILES[rootfs]="cramfs-xip.img"
ADDRS[tfa]=$ADDR_TFA
ADDRS[dtb]=$ADDR_DTB
ADDRS[kernel]=$ADDR_KERNEL
ADDRS[rootfs]=$ADDR_ROOTFS

# Determine which components to flash
if [[ -n "$COMPONENT" ]]; then
    if [[ -z "${FILES[$COMPONENT]+x}" ]]; then
        echo "Error: unknown component '$COMPONENT'. Use: tfa, dtb, kernel, rootfs"
        exit 1
    fi
    COMPONENTS=("$COMPONENT")
else
    COMPONENTS=(tfa dtb kernel rootfs)
fi

# Validate files exist
MISSING=false
for comp in "${COMPONENTS[@]}"; do
    file="$IMAGE_DIR/${FILES[$comp]}"
    if [[ ! -f "$file" ]]; then
        echo "Error: $file not found"
        MISSING=true
    fi
done
$MISSING && exit 1

# Print what we're doing
echo "=== Alif E7 MRAM Flash (J-Link) ==="
echo "Speed:  ${SPEED} kHz (~44 KB/s)"
echo "Images: $IMAGE_DIR"
echo ""

TOTAL_SIZE=0
for comp in "${COMPONENTS[@]}"; do
    file="$IMAGE_DIR/${FILES[$comp]}"
    size=$(stat -f%z "$file" 2>/dev/null || stat -c%s "$file" 2>/dev/null)
    TOTAL_SIZE=$((TOTAL_SIZE + size))
    printf "  %-10s %s @ %s (%s bytes)\n" "$comp" "${FILES[$comp]}" "${ADDRS[$comp]}" "$size"
done
echo ""
echo "Total: $((TOTAL_SIZE / 1024)) KB (est. $((TOTAL_SIZE / 1024 / 44 + 1)) seconds)"
echo ""

# Build JLink command script
JLINK_SCRIPT=$(mktemp /tmp/alif-jlink-XXXXXX.jlink)
trap "rm -f $JLINK_SCRIPT" EXIT

{
    for comp in "${COMPONENTS[@]}"; do
        file="$IMAGE_DIR/${FILES[$comp]}"
        echo "loadbin $file ${ADDRS[$comp]}"
    done

    if $VERIFY; then
        echo ""
        for comp in "${COMPONENTS[@]}"; do
            file="$IMAGE_DIR/${FILES[$comp]}"
            echo "verifybin $file ${ADDRS[$comp]}"
        done
    fi

    echo "exit"
} > "$JLINK_SCRIPT"

# Execute
JLinkExe -device "$DEVICE" -if SWD -speed "$SPEED" -autoconnect 1 -CommandFile "$JLINK_SCRIPT"

echo ""
echo "=== Flash complete ==="
echo "Power cycle the board (unplug/replug PRG_USB) to boot."
