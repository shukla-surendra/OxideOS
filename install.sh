#!/usr/bin/env bash
# install.sh — Write OxideOS to a USB drive or hard disk.
#
# Usage: sudo ./install.sh <device>
#   e.g. sudo ./install.sh /dev/sdb
#
# Build the image first with: make install-image
set -euo pipefail

IMAGE="oxide_install.img"
DEVICE="${1:-}"

if [[ -z "$DEVICE" ]]; then
    echo "Usage: $0 <device>  (e.g. /dev/sdb)"
    echo ""
    echo "Build the image first: make install-image"
    exit 1
fi

if [[ ! -f "$IMAGE" ]]; then
    echo "Error: $IMAGE not found."
    echo "Build it with: make install-image"
    exit 1
fi

if [[ ! -b "$DEVICE" ]]; then
    echo "Error: $DEVICE is not a block device."
    exit 1
fi

# Safety: refuse to write to mounted devices
if grep -q "^${DEVICE}" /proc/mounts 2>/dev/null; then
    echo "Error: $DEVICE (or a partition on it) is currently mounted."
    echo "Unmount it first and try again."
    exit 1
fi

IMAGE_BYTES=$(stat -c%s "$IMAGE")
DEVICE_BYTES=$(blockdev --getsize64 "$DEVICE" 2>/dev/null || echo 0)
IMAGE_MB=$(( IMAGE_BYTES / 1048576 ))
DEVICE_MB=$(( DEVICE_BYTES / 1048576 ))

if [[ "$DEVICE_BYTES" -lt "$IMAGE_BYTES" ]]; then
    echo "Error: device too small (${DEVICE_MB} MB < ${IMAGE_MB} MB required)."
    exit 1
fi

echo "╔══════════════════════════════════════════════════╗"
echo "║           OxideOS Installer                      ║"
echo "╚══════════════════════════════════════════════════╝"
echo ""
echo "  Image : $IMAGE  (${IMAGE_MB} MB)"
echo "  Target: $DEVICE (${DEVICE_MB} MB)"
echo ""
echo "  WARNING: ALL data on $DEVICE will be permanently erased!"
echo ""
read -rp "  Type 'yes' to continue, anything else to abort: " CONFIRM

if [[ "$CONFIRM" != "yes" ]]; then
    echo "Aborted."
    exit 1
fi

echo ""
echo "Writing OxideOS to $DEVICE ..."
dd if="$IMAGE" of="$DEVICE" bs=4M status=progress conv=fsync
sync

echo ""
echo "Done!  OxideOS is now installed on $DEVICE."
echo "You can boot from it on any x86_64 machine (BIOS or UEFI)."
