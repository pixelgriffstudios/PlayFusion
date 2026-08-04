#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

FLAVOR=${PLAYFUSION_FLAVOR:-full}
case "$FLAVOR" in
    full|lite) ;;
    *) echo "PLAYFUSION_FLAVOR must be full or lite" >&2; exit 2 ;;
esac

NAME=playfusion-1.0-public
ARCHIVE="/var/tmp/playfusion-release-output-${FLAVOR}/${NAME}.img.tar.xz"
SEED="/var/tmp/playfusion-release-output-${FLAVOR}/playfusion-seed"
IMAGE="/var/tmp/playfusion-release-verify-${FLAVOR}.img"
MOUNT="/var/tmp/playfusion-release-verify-${FLAVOR}"
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
test "$(realpath -m "$IMAGE")" = "/var/tmp/playfusion-release-verify-${FLAVOR}.img"
test "$(realpath -m "$MOUNT")" = "/var/tmp/playfusion-release-verify-${FLAVOR}"

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
if [[ "$FLAVOR" == full ]]; then
    test "$(find "$DEPLOYMENT/usr/share/kazeta/runtimes" -maxdepth 1 -type f -name '*.kzr' | wc -l)" -eq 39
    test -s "$DEPLOYMENT/usr/share/kazeta/runtimes/windows-1.2-experimental.kzr"
else
    test "$(find "$DEPLOYMENT/usr/share/kazeta/runtimes" -maxdepth 1 -type f -name '*.kzr' | wc -l)" -eq 1
    test -s "$DEPLOYMENT/usr/share/kazeta/runtimes/none.kzr"
fi
test "$(head -n 1 "$DEPLOYMENT/build_info")" = "$NAME"
grep -qx 'VERSION=1.0.3' "$DEPLOYMENT/etc/playfusion-release"
test -x "$DEPLOYMENT/usr/bin/playfusion-theme-splash"
test -x "$DEPLOYMENT/usr/bin/playfusion-update-helper"
test -x "$DEPLOYMENT/usr/bin/playfusion-update-health"
test -s "$DEPLOYMENT/etc/playfusion-update-public.pem"
test -L "$DEPLOYMENT/etc/systemd/system/graphical.target.wants/playfusion-update-health.service"
grep -q -- '--hwdec=vaapi-copy' "$DEPLOYMENT/usr/bin/playfusion-theme-splash"
test "$(readlink "$DEPLOYMENT/home/gamer/.local/share/kazeta-plus")" = \
    /var/kazeta/user-data/kazeta-plus
test ! -e "$DEPLOYMENT/home/gamer/.cargo"
test ! -e "$DEPLOYMENT/home/gamer/.rustup"
test -d "$DEPLOYMENT/home/gamer/.local/share/waydroid"
test -z "$(find "$DEPLOYMENT/home/gamer/.local/share/waydroid" -mindepth 1 -print -quit)"
test ! -s "$DEPLOYMENT/etc/machine-id"
test -z "$(find "$DEPLOYMENT/etc/NetworkManager/system-connections" -mindepth 1 -print -quit 2>/dev/null)"
test -z "$(getfattr --absolute-names -R -h -m '^btrfs\.compression$' -d \
    "$DEPLOYMENT" 2>/dev/null | grep '^# file:' || true)"

test -f "$SEED/internal-games/Arcade/internal-playfusion-arcade/cart.kzi"
test "$(stat -c '%u:%g' "$SEED")" = 1000:1000
test "$(stat -c '%a' "$SEED")" = 755
test -f "$SEED/internal-games/PC Games/internal-hell-on-rails/cart.kzi"
test "$(find "$SEED/internal-games" -mindepth 2 -maxdepth 2 -type d | wc -l)" -eq 2
test -d "$SEED/music/30 years"
test "$(find "$SEED/user-data/kazeta-plus/themes" -mindepth 1 -maxdepth 1 -type d | wc -l)" -eq 2
for theme in xbox_original xbox_2_0; do
    test -s "$SEED/user-data/kazeta-plus/themes/$theme/theme.toml"
    test -s "$SEED/user-data/kazeta-plus/themes/$theme/boot_animation.mp4"
done
grep -q '^resolution = "1280x720"$' "$SEED/user-data/kazeta-plus/config.toml"
grep -q '^theme = "Default"$' "$SEED/user-data/kazeta-plus/config.toml"
grep -q '^background_selection = "ProjectM Fusion"$' "$SEED/user-data/kazeta-plus/config.toml"
test -z "$(find "$SEED/movies" -mindepth 1 -print -quit)"
test -z "$(find "$SEED/firmware" -type f -print -quit)"
test "$(cat "$SEED/active-profile")" = default
test "$(find "$SEED/profiles" -maxdepth 1 -type f | wc -l)" -eq 1
test "$(grep '^name = ' "$SEED/profiles/default.toml")" = 'name = "Default"'
test -z "$(find "$SEED/saves" -type f -print -quit)"
test "$(find "$SEED/state" -mindepth 1 -maxdepth 1 | wc -l)" -eq 3
test -d "$SEED/state/projectm-home"
test -d "$SEED/state/wireplumber"
test -f "$SEED/state/playfusion-favorites"

"$SCRIPT_DIR/verify-public-rootfs.sh" "$DEPLOYMENT"
"$SCRIPT_DIR/verify-public-rootfs.sh" "$SEED"

echo ARCHIVE_VERIFY_OK
du -sh "$ARCHIVE" "$DEPLOYMENT/usr/share/kazeta/runtimes" "$SEED"
sha256sum "$ARCHIVE"
