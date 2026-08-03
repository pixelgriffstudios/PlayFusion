#!/usr/bin/env bash
set -Eeuo pipefail

DEP=/var/tmp/playfusion-102-fixed-receive/playfusion-1.0-public
KIT=/var/tmp/pf-release-v102/out/PlayFusion-legacy-update-v1.0.2
PAYLOAD=/var/tmp/pf-release-v102/payload
SEED=/var/tmp/playfusion-release-output/playfusion-seed
THEME_STAGE=/var/tmp/playfusion-102-theme-seed

test "$(id -u)" -eq 0
test -d "$DEP"
test -d "$KIT"
test -d "$PAYLOAD"
test -d "$SEED"
test -s "$KIT/playfusion-update-helper"
test -s "$PAYLOAD/usr/bin/kazeta-bios"

reseal() {
    btrfs property set -ts "$DEP" ro true >/dev/null 2>&1 || true
}
trap reseal ERR

btrfs property set -f -ts "$DEP" ro false
cp -a -- "$PAYLOAD/." "$DEP/"

install -D -o root -g root -m 0755 \
    "$KIT/playfusion-update-helper" "$DEP/usr/bin/playfusion-update-helper"
install -D -o root -g root -m 0755 \
    "$KIT/playfusion-update-health" "$DEP/usr/bin/playfusion-update-health"
install -D -o root -g root -m 0644 \
    "$KIT/playfusion-update-public.pem" "$DEP/etc/playfusion-update-public.pem"
install -D -o root -g root -m 0644 \
    "$KIT/playfusion-update-health.service" \
    "$DEP/etc/systemd/system/playfusion-update-health.service"
install -D -o root -g root -m 0440 \
    "$KIT/playfusion-update.sudoers" "$DEP/etc/sudoers.d/playfusion-update"
printf 'PRODUCT=PlayFusion\nVERSION=1.0.2\n' > "$DEP/etc/playfusion-release"
chown root:root "$DEP/etc/playfusion-release"
chmod 0644 "$DEP/etc/playfusion-release"

# Copy the complete, hardware-tested PS1 optical stack from the live system.
for path in \
    /usr/bin/kazeta-optical-disc-helper \
    /usr/bin/kazeta-ps1-identify \
    /etc/systemd/system/optical-mount@.service \
    /etc/systemd/system/optical-unmount@.service \
    /etc/udev/rules.d/60-kazeta-optical-fast.rules \
    /etc/udev/rules.d/99-optical-automount.rules \
    /usr/share/kazeta/runtimes/playstation-1.01.kzr \
    /usr/share/kazeta/runtimes/playstation-info.txt \
    /usr/share/playfusion/databases/ps1-titles.tsv \
    /usr/share/kazeta/optical-icons/ps1.png; do
    test -s "$path"
    cp -a --parents -- "$path" "$DEP"
done

# Each installation generates unique SSH host keys after deployment.
rm -f -- "$DEP/etc/ssh/ssh_host_"*

test "$(sha256sum "$DEP/usr/bin/kazeta-bios" | awk '{print $1}')" = \
    0b8d931e7eaaf5a8c71011b10bb3d3b38c74ac1c0d870844ead8685cc1cf910f
test "$(sha256sum "$DEP/usr/bin/kazeta-optical-disc-helper" | awk '{print $1}')" = \
    786bff626472ce565e68f33a765b52ae2b6ca0e6a09b2ace66830042b8f5f82a
test "$(sha256sum "$DEP/usr/share/kazeta/runtimes/playstation-1.01.kzr" | awk '{print $1}')" = \
    78fa03e7f75a400d5deaec7e3f2c335ccbfcdec5fbf0bf068a537b08f1b673bc
test "$(stat -c '%U:%G:%a' "$DEP/usr/bin/kazeta-bios")" = root:root:755
test "$(stat -c '%U:%G:%a' "$DEP/etc/sudoers.d/playfusion-update")" = root:root:440

btrfs property set -ts "$DEP" ro true
trap - ERR

# Seed both complete Xbox themes into clean installations. The signed theme
# archives remain in /usr/share for repair/reinstallation after first boot.
rm -rf -- "$THEME_STAGE"
mkdir -p "$THEME_STAGE" "$SEED/user-data/kazeta-plus/themes"
bsdtar --no-same-owner -xf \
    "$DEP/usr/share/playfusion/themes/xbox_original-PlayFusion-optimized.zip" \
    -C "$THEME_STAGE"
bsdtar --no-same-owner -xf \
    "$DEP/usr/share/playfusion/themes/xbox_2_0-PlayFusion.zip" \
    -C "$THEME_STAGE"
test -s "$THEME_STAGE/xbox_original/theme.toml"
test -s "$THEME_STAGE/xbox_original/boot_animation.mp4"
test -s "$THEME_STAGE/xbox_2_0/theme.toml"
test -s "$THEME_STAGE/xbox_2_0/boot_animation.mp4"
cp -a -- "$THEME_STAGE/xbox_original" "$SEED/user-data/kazeta-plus/themes/"
cp -a -- "$THEME_STAGE/xbox_2_0" "$SEED/user-data/kazeta-plus/themes/"
chown -R 1000:1000 "$SEED/user-data"

test "$(find "$SEED/user-data/kazeta-plus/themes" \
    -mindepth 1 -maxdepth 1 -type d | wc -l)" -eq 2
test -z "$(find "$SEED/saves" -type f -print -quit)"

printf '%s\n' PLAYFUSION_1_0_2_DEPLOYMENT_PATCH_OK
sha256sum \
    "$DEP/usr/bin/kazeta-bios" \
    "$DEP/usr/bin/playfusion-update-helper" \
    "$DEP/usr/bin/playfusion-update-health" \
    "$DEP/usr/bin/kazeta-optical-disc-helper" \
    "$DEP/usr/share/kazeta/runtimes/playstation-1.01.kzr"
