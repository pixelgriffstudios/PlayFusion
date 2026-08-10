#!/usr/bin/env bash
set -Eeuo pipefail

IMAGE=${1:?Usage: audit-playfusion-installer.sh IMAGE}
test -r "$IMAGE"

MOUNT_DIR=$(mktemp -d /var/tmp/playfusion-installer-audit.XXXXXX)
cleanup() {
    if mountpoint -q "$MOUNT_DIR"; then
        umount "$MOUNT_DIR"
    fi
    rmdir "$MOUNT_DIR" 2>/dev/null || true
}
trap cleanup EXIT

# The installer is a hybrid GPT/ISO image.  Mounting the ISO9660 view read-only
# exposes the deployment archive without modifying either GPT copy.
mount -o loop,ro -t iso9660 -- "$IMAGE" "$MOUNT_DIR"
printf 'Mounted installer read-only at %s\n' "$MOUNT_DIR"
find "$MOUNT_DIR" -maxdepth 3 -printf '%y %10s %P\n' | sort

ROOTFS="$MOUNT_DIR/arch/x86_64/airootfs.sfs"
test -s "$ROOTFS"
# WSL's kernel may lack SquashFS Zstandard support even when user-space tools
# support it, so inspect the compressed root with unsquashfs instead of mounting
# it.  This remains read-only and works on the same Zstandard image PCs boot.
printf '\nInstaller root filesystem entries relevant to PlayFusion:\n'
unsquashfs -ll "$ROOTFS" 2>/dev/null | \
    grep -Ei 'playfusion|deployment|installer|airootfs/rootfs|sudoers|sshd_config' | \
    head -n 500

printf '\nFirst entries in the embedded installed-system archive:\n'
set +o pipefail
unsquashfs -cat "$ROOTFS" root/playfusion-1.0-public.img.tar.xz 2>/dev/null | \
    tar -tJf - 2>/dev/null | head -n 120
set -o pipefail
