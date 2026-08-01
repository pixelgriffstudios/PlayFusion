#!/usr/bin/env bash
set -euo pipefail

WORK=/var/tmp/install-media
PROFILE="$WORK/installer/profiledef.sh"
AIROOT="$WORK/installer/airootfs"

test "$(id -u)" -eq 0
test -f "$PROFILE"
test -f /home/gamer/playfusion-public-installer.sh
test -f /var/tmp/playfusion-release-output/kazeta-2025-0_545b900.img.tar.xz
test -d /var/tmp/playfusion-release-output/playfusion-seed

install -m 0755 /home/gamer/playfusion-public-installer.sh \
    "$AIROOT/root/install.sh"

rm -f "$AIROOT/root"/*.img.tar.xz
cp --reflink=auto \
    /var/tmp/playfusion-release-output/kazeta-2025-0_545b900.img.tar.xz \
    "$AIROOT/root/kazeta-2025-0_545b900.img.tar.xz"

rm -rf "$AIROOT/root/playfusion-seed"
rm -f "$AIROOT/root/playfusion-seed.tar"
# ArchISO's SquashFS normalizes directory ownership.  Store the factory tree
# as a tar archive so numeric owners, modes, ACLs, and xattrs survive intact.
tar --numeric-owner --acls --xattrs \
    -C /var/tmp/playfusion-release-output/playfusion-seed \
    -cpf "$AIROOT/root/playfusion-seed.tar" .

sed -i \
    -e 's/^iso_name=.*/iso_name="playfusion"/' \
    -e 's/^iso_label=.*/iso_label="PLAYFUSION_1_0"/' \
    -e 's|^iso_publisher=.*|iso_publisher="PlayFusion <https://github.com/kazetaos>"|' \
    -e 's/^iso_application=.*/iso_application="PlayFusion 1.0 Installer"/' \
    "$PROFILE"

# The large deployment payload is already XZ-compressed.  Zstd keeps the live
# filesystem quick to build and quick to boot without wasting time trying to
# recompress the archive a second time.
sed -i \
    "s/airootfs_image_tool_options=.*/airootfs_image_tool_options=('-comp' 'zstd' '-Xcompression-level' '3' '-b' '1M')/" \
    "$PROFILE"

sed -i 's/title   Install Kazeta/title   Install PlayFusion 1.0/' \
    "$WORK/installer/efiboot/loader/entries/archiso-x86_64-linux.conf"

sed -i \
    -e 's/^NAME=.*/NAME="PlayFusion Installer"/' \
    -e 's/^PRETTY_NAME=.*/PRETTY_NAME="PlayFusion 1.0 Installer"/' \
    "$AIROOT/etc/os-release"

# The installed PlayFusion deployment intentionally uses Kazeta's frozen
# archive mirror for reproducible unlocks.  The live installer itself must use
# current Arch packages and signing keys, otherwise revoked historical signing
# keys can prevent the UEFI environment from being assembled.
sed -i \
    's|^Include = /etc/pacman.d/mirrorlist$|Server = https://geo.mirror.pkgbuild.com/$repo/os/$arch|' \
    "$WORK/installer/pacman.conf"

# Prevent the release payload and approved games from being accidentally
# altered while mkarchiso is collecting the live filesystem.
chmod 0444 "$AIROOT/root/kazeta-2025-0_545b900.img.tar.xz"
chmod 0444 "$AIROOT/root/playfusion-seed.tar"

echo "Install media staged:"
du -sh "$AIROOT/root/kazeta-2025-0_545b900.img.tar.xz" \
    "$AIROOT/root/playfusion-seed.tar"
sha256sum "$AIROOT/root/kazeta-2025-0_545b900.img.tar.xz"
