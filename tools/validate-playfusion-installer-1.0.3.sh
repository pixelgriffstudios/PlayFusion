#!/usr/bin/env bash
set -euo pipefail

FLAVOR=${PLAYFUSION_FLAVOR:-full}
case "$FLAVOR" in
    full) LABEL=Full; EXPECTED_RUNTIMES=39 ;;
    lite) LABEL=Lite; EXPECTED_RUNTIMES=1 ;;
    *) echo "PLAYFUSION_FLAVOR must be full or lite" >&2; exit 2 ;;
esac

IMG="/var/tmp/PlayFusion-1.0.3-${LABEL}-Installer.img"
WORK="/var/tmp/playfusion-103-repack-${FLAVOR}"
SOURCE_DEPLOYMENT="/var/tmp/playfusion-release-output-${FLAVOR}/playfusion-1.0-public.img.tar.xz"
EMBEDDED_DEPLOYMENT="$WORK/airootfs/root/playfusion-1.0-public.img.tar.xz"
SEED_VERIFY="/var/tmp/playfusion-103-seed-verify-${FLAVOR}"
VERIFY=/home/gamer/verify-public-rootfs.sh

test "$(id -u)" -eq 0
test -s "$IMG"
test -s "$SOURCE_DEPLOYMENT"
test -s "$EMBEDDED_DEPLOYMENT"

toc=$(xorriso -indev "$IMG" -toc 2>&1)
grep -q 'Boot record  : El Torito , MBR protective-msdos-label.*GPT' <<<"$toc"
grep -q "Volume id    : 'PLAYFUSION_1_0'" <<<"$toc"
(cd "$WORK" && sha512sum -c airootfs.sha512)
test "$(sha256sum "$EMBEDDED_DEPLOYMENT" | awk '{print $1}')" = \
    "$(sha256sum "$SOURCE_DEPLOYMENT" | awk '{print $1}')"
test "$(cat "$WORK/airootfs/root/playfusion-installer-flavor")" = "$FLAVOR"

rm -rf -- "$SEED_VERIFY"
mkdir -p "$SEED_VERIFY"
tar --numeric-owner --acls --xattrs -xpf \
    "$WORK/airootfs/root/playfusion-seed.tar" -C "$SEED_VERIFY"
"$VERIFY" "$SEED_VERIFY"
test "$(find "$SEED_VERIFY/internal-games" -mindepth 2 -maxdepth 2 -type d | wc -l)" -eq 2
test -z "$(find "$SEED_VERIFY/firmware" -type f -print -quit)"
test -z "$(find "$SEED_VERIFY/saves" -type f -print -quit)"

printf 'IMAGE_VALIDATION_OK flavor=%s expected_runtimes=%s\n' "$FLAVOR" "$EXPECTED_RUNTIMES"
sha256sum "$IMG" "$SOURCE_DEPLOYMENT"
