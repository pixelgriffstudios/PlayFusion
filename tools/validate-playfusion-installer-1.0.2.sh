#!/usr/bin/env bash
set -euo pipefail

IMG=/var/tmp/PlayFusion-1.0.2-Public-Installer-Fixed.img
WORK=/var/tmp/playfusion-102-repack
SOURCE_DEPLOYMENT=/var/tmp/playfusion-102-deployment-fixed/kazeta-2025-0_545b900.img.tar.xz
EMBEDDED_DEPLOYMENT="$WORK/airootfs/root/kazeta-2025-0_545b900.img.tar.xz"
SEED_VERIFY=/var/tmp/playfusion-102-seed-verify

test "$(id -u)" -eq 0
test -s "$IMG"
test -s "$SOURCE_DEPLOYMENT"
test -s "$EMBEDDED_DEPLOYMENT"

toc=$(xorriso -indev "$IMG" -toc 2>&1)
grep -q 'Boot record  : El Torito , MBR protective-msdos-label.*GPT' <<<"$toc"
grep -q "Volume id    : 'PLAYFUSION_1_0'" <<<"$toc"
printf '%s\n' "$toc" | grep -E 'Boot record|Volume id|Media summary'

(cd "$WORK" && sha512sum -c airootfs.sha512)
test "$(sha256sum "$EMBEDDED_DEPLOYMENT" | awk '{print $1}')" = \
    "$(sha256sum "$SOURCE_DEPLOYMENT" | awk '{print $1}')"
grep -q 'PlayFusion 1.0.2 installed successfully' "$WORK/airootfs/root/install.sh"

rm -rf -- "$SEED_VERIFY"
mkdir -p "$SEED_VERIFY"
tar --numeric-owner --acls --xattrs -xpf \
    "$WORK/airootfs/root/playfusion-seed.tar" -C "$SEED_VERIFY"
test "$(find "$SEED_VERIFY/internal-games" \
    -mindepth 2 -maxdepth 2 -type d | wc -l)" -eq 2
test "$(find "$SEED_VERIFY/user-data/kazeta-plus/themes" \
    -mindepth 1 -maxdepth 1 -type d | wc -l)" -eq 2
test -s "$SEED_VERIFY/user-data/kazeta-plus/themes/xbox_original/boot_animation.mp4"
test -s "$SEED_VERIFY/user-data/kazeta-plus/themes/xbox_2_0/boot_animation.mp4"
test -z "$(find "$SEED_VERIFY/saves" -type f -print -quit)"

printf '%s\n' IMAGE_VALIDATION_OK
sha256sum "$IMG" "$SOURCE_DEPLOYMENT"
