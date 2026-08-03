#!/usr/bin/env bash
set -euo pipefail

# Reuse the real-hardware-tested 1.0.2 hybrid ISO/IMG boot shell and replace
# only its corrected deployment, factory seed, and installer UI.
BASE=/var/tmp/PlayFusion-1.0.2-base.img
OUTPUT=/var/tmp/PlayFusion-1.0.2-Public-Installer-Fixed.img
RELEASE=/var/tmp/playfusion-release-output
DEPLOYMENT=/var/tmp/playfusion-102-deployment-fixed/kazeta-2025-0_545b900.img.tar.xz
SEED="$RELEASE/playfusion-seed"
INSTALLER=/home/gamer/playfusion-public-installer-1.0.2.sh
WORK=/var/tmp/playfusion-102-repack
ISO_MOUNT="$WORK/base-iso"
AIROOT="$WORK/airootfs"
NEW_SFS="$WORK/airootfs.sfs"
NEW_SHA="$WORK/airootfs.sha512"
LOADER_ENTRY="$WORK/archiso-x86_64-linux.conf"

cleanup() {
    if mountpoint -q "$ISO_MOUNT"; then
        umount "$ISO_MOUNT" || true
    fi
}
trap cleanup EXIT

test "$(id -u)" -eq 0
test "$(realpath -m "$WORK")" = /var/tmp/playfusion-102-repack
test "$(realpath -m "$OUTPUT")" = /var/tmp/PlayFusion-1.0.2-Public-Installer-Fixed.img
test -s "$BASE"
test -s "$DEPLOYMENT"
test -d "$SEED"
test -s "$INSTALLER"
command -v xorriso >/dev/null
command -v unsquashfs >/dev/null
command -v mksquashfs >/dev/null

rm -rf -- "$WORK"
rm -f -- "$OUTPUT" "$OUTPUT.sha256"
mkdir -p "$ISO_MOUNT" "$AIROOT"

echo '[1/7] Mounting the proven bootable 1.0.1 installer shell...'
mount -o loop,ro "$BASE" "$ISO_MOUNT"
test -s "$ISO_MOUNT/arch/x86_64/airootfs.sfs"
test -s "$ISO_MOUNT/arch/x86_64/airootfs.sha512"

echo '[2/7] Extracting the installer filesystem...'
unsquashfs -no-progress -d "$AIROOT" "$ISO_MOUNT/arch/x86_64/airootfs.sfs"

echo '[3/7] Installing the clean 1.0.2 payload and factory seed...'
rm -f -- "$AIROOT/root"/*.img.tar.xz "$AIROOT/root/playfusion-seed.tar"
install -m 0444 "$DEPLOYMENT" "$AIROOT/root/kazeta-2025-0_545b900.img.tar.xz"
tar --numeric-owner --acls --xattrs -C "$SEED" -cpf "$AIROOT/root/playfusion-seed.tar" .
chmod 0444 "$AIROOT/root/playfusion-seed.tar"
install -m 0755 "$INSTALLER" "$AIROOT/root/install.sh"
sed -i \
    -e 's/^NAME=.*/NAME="PlayFusion Installer"/' \
    -e 's/^PRETTY_NAME=.*/PRETTY_NAME="PlayFusion 1.0.2 Installer"/' \
    "$AIROOT/etc/os-release"

echo '[4/7] Validating installer contents before compression...'
test "$(find "$AIROOT/root" -maxdepth 1 -type f -name '*.img.tar.xz' | wc -l)" -eq 1
grep -q 'PlayFusion 1.0.2 installed successfully' "$AIROOT/root/install.sh"
test "$(find "$SEED/internal-games" -mindepth 2 -maxdepth 2 -type d | wc -l)" -eq 2
test "$(find "$SEED/user-data/kazeta-plus/themes" -mindepth 1 -maxdepth 1 -type d | wc -l)" -eq 2
test -z "$(find "$SEED/saves" -type f -print -quit)"

echo '[5/7] Rebuilding the installer SquashFS...'
mksquashfs "$AIROOT" "$NEW_SFS" -noappend -no-progress \
    -comp zstd -Xcompression-level 3 -b 1M
(cd "$WORK" && sha512sum airootfs.sfs > airootfs.sha512)

cp "$ISO_MOUNT/loader/entries/archiso-x86_64-linux.conf" "$LOADER_ENTRY"
sed -i -E 's/^title[[:space:]]+.*/title   Install PlayFusion 1.0.2/' "$LOADER_ENTRY"

echo '[6/7] Replaying the tested hybrid boot layout with the new payload...'
xorriso \
    -indev "$BASE" \
    -outdev "$OUTPUT" \
    -boot_image any replay \
    -map "$NEW_SFS" /arch/x86_64/airootfs.sfs \
    -map "$NEW_SHA" /arch/x86_64/airootfs.sha512 \
    -map "$LOADER_ENTRY" /loader/entries/archiso-x86_64-linux.conf \
    -commit

echo '[7/7] Verifying the final hybrid installer...'
test -s "$OUTPUT"
xorriso -indev "$OUTPUT" -toc >/dev/null 2>&1
sha256sum "$OUTPUT" | tee "$OUTPUT.sha256"
printf 'BASE_SIZE=%s\n' "$(stat -c %s "$BASE")"
printf 'OUTPUT_SIZE=%s\n' "$(stat -c %s "$OUTPUT")"
echo PLAYFUSION_1_0_2_REPACK_OK
