#!/bin/bash
# Install Alif E7 JLink device definition into SEGGER's user directory.
# This enables JLinkExe to recognize the E7 and write MRAM via loadbin.
# The JLinkScript prevents reset (which would kill SE boot sequence).
#
# Run once after cloning the repo or updating files.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DEST_DIR="$HOME/Library/Application Support/SEGGER/JLinkDevices/AlifSemi"

echo "Installing Alif E7 JLink device definition..."
echo "  Source: $SCRIPT_DIR"
echo "  Dest:   $DEST_DIR"

mkdir -p "$DEST_DIR"
cp "$SCRIPT_DIR/Devices.xml" "$DEST_DIR/"
cp "$SCRIPT_DIR/AlifE7.JLinkScript" "$DEST_DIR/"

echo "Done. JLinkExe will now recognize AE722F80F55D5_M55_HP with MRAM flash."
echo ""
echo "Test with: JLinkExe -device AE722F80F55D5_M55_HP -if SWD -speed 4000 -autoconnect 1"
