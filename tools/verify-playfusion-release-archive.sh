#!/usr/bin/env bash
set -euo pipefail

NAME=kazeta-2025-0_545b900
ARCHIVE="/var/tmp/playfusion-release-output/${NAME}.img.tar.xz"
SEED=/var/tmp/playfusion-release-output/playfusion-seed
IMAGE=/var/tmp/playfusion-release-verify.img
MOUNT=/var/tmp/playfusion-release-verify
LOOP=

cleanup() {
    if mountpoint -q "$MOUNT"; then umount "$MOUNT"; fi
    if test -n "${LOOP}"; then losetup -d "$LOOP" 2>/dev/null || true; fi
    rm -f -- "$IMAGE"
    rmdir "$MOUNT" 2>/dev/null || true
}
trap cleanup EXIT

test "$(id -u)" -eq 0
test -f "$ARCHIVE"
test -d "$SEED"
test "$(realpath -m "$IMAGE")" = /var/tmp/playfusion-release-verify.img
test "$(realpath -m "$MOUNT")" = /var/tmp/playfusion-release-verify

# The receive pass below validates the complete XZ, tar, and Btrfs streams.
# Avoid a separate list/test pass that would decompress this 8 GB archive twice.
xz -l "$ARCHIVE" >/dev/null

mkdir -p "$MOUNT"
truncate -s 28G "$IMAGE"
LOOP=$(losetup --find --show "$IMAGE")
mkfs.btrfs -q -f "$LOOP"
mount -t btrfs "$LOOP" "$MOUNT"
tar -xf "$ARCHIVE" -O | btrfs receive "$MOUNT"

DEPLOYMENT="$MOUNT/$NAME"
test -d "$DEPLOYMENT"
test "$(find "$DEPLOYMENT/usr/share/kazeta/runtimes" -maxdepth 1 -type f -name '*.kzr' | wc -l)" -eq 38
test "$(head -n 1 "$DEPLOYMENT/build_info")" = "$NAME"
test ! -e "$DEPLOYMENT/home/gamer/.cargo"
test ! -e "$DEPLOYMENT/home/gamer/.rustup"
test ! -s "$DEPLOYMENT/etc/machine-id"
test -z "$(find "$DEPLOYMENT/etc/NetworkManager/system-connections" -mindepth 1 -print -quit 2>/dev/null)"

test -f "$SEED/internal-games/Arcade/internal-playfusion-arcade/cart.kzi"
test "$(stat -c '%u:%g' "$SEED")" = 1000:1000
test "$(stat -c '%a' "$SEED")" = 755
test -f "$SEED/internal-games/PC Games/internal-hell-on-rails/cart.kzi"
test "$(find "$SEED/internal-games" -mindepth 2 -maxdepth 2 -type d | wc -l)" -eq 2
test -d "$SEED/music/30 years"
test -z "$(find "$SEED/movies" -mindepth 1 -print -quit)"
test -n "$(find "$SEED/firmware" -type f -print -quit)"
test "$(cat "$SEED/active-profile")" = default
test "$(find "$SEED/profiles" -maxdepth 1 -type f | wc -l)" -eq 1
test "$(grep '^name = ' "$SEED/profiles/default.toml")" = 'name = "Default"'
test -z "$(find "$SEED/saves" -type f -print -quit)"

echo ARCHIVE_VERIFY_OK
du -sh "$ARCHIVE" "$DEPLOYMENT/usr/share/kazeta/runtimes" "$SEED"
sha256sum "$ARCHIVE"
