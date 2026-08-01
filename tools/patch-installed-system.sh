#!/bin/bash
set -euo pipefail

exec > >(tee /dev/ttyS0) 2>&1

echo "=== Kazeta+ Internal Games installed-system patch ==="

TARGET_DEVICE="${1:-/dev/vda2}"
PATCH_ROOT="${PATCH_ROOT:-/codex-patch}"
MOUNT_PATH=/mnt/kazeta-target

if [ ! -b "$TARGET_DEVICE" ]; then
    echo "ERROR: Target BTRFS partition was not found: $TARGET_DEVICE"
    poweroff
    exit 1
fi

for required in \
    "$PATCH_ROOT/kazeta-bios" \
    "$PATCH_ROOT/covers/internal-super-mario-all-stars-world/cover.jpg" \
    "$PATCH_ROOT/covers/internal-beyond-the-beyond/cover.jpg" \
    "$PATCH_ROOT/rootfs/etc/kazeta-update-public.pem" \
    "$PATCH_ROOT/rootfs/usr/bin/kazeta-internal-game-helper" \
    "$PATCH_ROOT/rootfs/usr/bin/kazeta-import-ftp-runtimes"; do
    if [ ! -f "$required" ]; then
        echo "ERROR: Patch payload is missing: $required"
        poweroff
        exit 1
    fi
done

mkdir -p "$MOUNT_PATH"
mount -o subvolid=5 "$TARGET_DEVICE" "$MOUNT_PATH"

DEPLOYMENT_DIR=$(find "$MOUNT_PATH/deployments" \
    -mindepth 1 -maxdepth 1 -type d -name 'kazeta-*' -print -quit)
if [ -z "$DEPLOYMENT_DIR" ]; then
    echo "ERROR: Kazeta deployment was not found."
    umount "$MOUNT_PATH"
    poweroff
    exit 1
fi

echo "Patching deployment: $DEPLOYMENT_DIR"
btrfs property set -ts "$DEPLOYMENT_DIR" ro false

cp -a "$PATCH_ROOT/rootfs/." "$DEPLOYMENT_DIR/"
install -o root -g root -m 755 \
    "$PATCH_ROOT/kazeta-bios" \
    "$DEPLOYMENT_DIR/usr/bin/kazeta-bios"

while IFS= read -r -d '' source_file; do
    program=$(basename "$source_file")
    chown root:root "$DEPLOYMENT_DIR/usr/bin/$program"
    chmod 755 "$DEPLOYMENT_DIR/usr/bin/$program"
done < <(find "$PATCH_ROOT/rootfs/usr/bin" -maxdepth 1 -type f -print0)

chown -R root:root \
    "$DEPLOYMENT_DIR/etc/systemd/system" \
    "$DEPLOYMENT_DIR/etc/sudoers.d"
find "$DEPLOYMENT_DIR/etc/systemd/system" -type f -exec chmod 644 {} \;
find "$DEPLOYMENT_DIR/etc/sudoers.d" -type f -exec chmod 440 {} \;
chmod 644 \
    "$DEPLOYMENT_DIR/etc/kazeta-update-public.pem" \
    "$DEPLOYMENT_DIR/etc/vsftpd-kazeta-internal.conf" \
    "$DEPLOYMENT_DIR/usr/lib/sysusers.d/kazeta-internal-games.conf"

systemd-sysusers \
    --root="$DEPLOYMENT_DIR" \
    "$DEPLOYMENT_DIR/usr/lib/sysusers.d/kazeta-internal-games.conf"

FTP_UID=$(chroot "$DEPLOYMENT_DIR" /usr/bin/id -u kazetaftp)
FTP_GID=$(chroot "$DEPLOYMENT_DIR" /usr/bin/id -g kazetaftp)

mkdir -p \
    "$MOUNT_PATH/var/kazeta/internal-games" \
    "$MOUNT_PATH/var/kazeta/runtime-uploads" \
    "$MOUNT_PATH/var/kazeta/updates" \
    "$MOUNT_PATH/var/kazeta/ftp/internal-games" \
    "$MOUNT_PATH/var/kazeta/ftp/runtimes" \
    "$MOUNT_PATH/var/kazeta/ftp/updates"

# The immutable deployment contains the package-created /var skeleton, but the
# installed system mounts a separate persistent BTRFS /var subvolume. Populate
# that subvolume once so LightDM, PAM, pacman, and system services have their
# expected state directories. Preserve games and other data already written.
(
    cd "$DEPLOYMENT_DIR/var"
    tar --exclude='./cache/pacman/pkg/*' -cpf - .
) | (
    cd "$MOUNT_PATH/var"
    tar --skip-old-files -xpf -
)

mkdir -p \
    "$MOUNT_PATH/var/lib/lightdm-data" \
    "$MOUNT_PATH/var/log/lightdm" \
    "$MOUNT_PATH/var/log/lastlog" \
    "$MOUNT_PATH/var/tmp"
chmod 1770 "$MOUNT_PATH/var/lib/lightdm-data"
chmod 1777 "$MOUNT_PATH/var/tmp"

LIGHTDM_UID=$(chroot "$DEPLOYMENT_DIR" /usr/bin/id -u lightdm)
LIGHTDM_GID=$(chroot "$DEPLOYMENT_DIR" /usr/bin/id -g lightdm)
chown "$LIGHTDM_UID:$LIGHTDM_GID" \
    "$MOUNT_PATH/var/lib/lightdm-data" \
    "$MOUNT_PATH/var/log/lightdm"

# The previous combined installer changed ownership on every /usr/bin file,
# which caused Linux to clear required setuid bits. Restore the standard modes
# used by login, sudo, policy, and removable-media helpers.
for setuid_program in sudo su passwd chfn chsh gpasswd newgrp newuidmap newgidmap pkexec mount umount; do
    if [ -e "$DEPLOYMENT_DIR/usr/bin/$setuid_program" ]; then
        chmod 4755 "$DEPLOYMENT_DIR/usr/bin/$setuid_program"
    fi
done

install -m 644 \
    "$PATCH_ROOT/covers/internal-super-mario-all-stars-world/cover.jpg" \
    "$MOUNT_PATH/var/kazeta/internal-games/super-mario-all-stars-world/cover.jpg"
install -m 644 \
    "$PATCH_ROOT/covers/internal-beyond-the-beyond/cover.jpg" \
    "$MOUNT_PATH/var/kazeta/internal-games/beyond-the-beyond/cover.jpg"

chown -R "$FTP_UID:$FTP_GID" \
    "$MOUNT_PATH/var/kazeta/internal-games" \
    "$MOUNT_PATH/var/kazeta/runtime-uploads" \
    "$MOUNT_PATH/var/kazeta/updates" \
    "$MOUNT_PATH/var/kazeta/ftp"

find "$MOUNT_PATH/var/kazeta" -type d -exec chmod 755 {} \;

systemctl --root="$DEPLOYMENT_DIR" enable kazeta-internal-ftp.service
systemctl --root="$DEPLOYMENT_DIR" enable kazeta-import-ftp-runtimes.path

echo "Verifying installed files and services..."
test -x "$DEPLOYMENT_DIR/usr/bin/kazeta-bios"
test -x "$DEPLOYMENT_DIR/usr/bin/kazeta-internal-game-helper"
test -x "$DEPLOYMENT_DIR/usr/bin/kazeta-import-ftp-runtimes"
test -L "$DEPLOYMENT_DIR/etc/systemd/system/multi-user.target.wants/kazeta-internal-ftp.service"
test -L "$DEPLOYMENT_DIR/etc/systemd/system/multi-user.target.wants/kazeta-import-ftp-runtimes.path"

btrfs property set -ts "$DEPLOYMENT_DIR" ro true
sync
umount "$MOUNT_PATH"

echo "=== PATCH COMPLETE ==="
poweroff
