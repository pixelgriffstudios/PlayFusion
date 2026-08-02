#!/usr/bin/env bash
set -euo pipefail

THEME_ROOT=/var/kazeta/user-data/kazeta-plus/themes
SOURCE_THEME="$THEME_ROOT/xbox_original"
XBOX_2_THEME="$THEME_ROOT/xbox_2_0"
BUILD_ROOT=/var/kazeta/state/bios-android-build/bios
BACKUP_ROOT=/var/kazeta/state/playfusion-pre-102-backup

test -d "$SOURCE_THEME"
test -s "$SOURCE_THEME/boot_animation.mp4"
test -x "$BUILD_ROOT/target/release/kazeta-bios"
test -s /home/gamer/pf-stage-rootfs-usr-bin-playfusion-theme-splash
test -s /home/gamer/pf-stage-rootfs-usr-bin-kazeta-session

# Xbox 2.0 is a separate native-rendered theme. Refuse to overwrite an
# unexpected directory; a previous generated copy is safe to refresh.
if test -e "$XBOX_2_THEME"; then
    test -f "$XBOX_2_THEME/.playfusion-generated-xbox-2"
    rm -rf -- "$XBOX_2_THEME"
fi
cp -a -- "$SOURCE_THEME" "$XBOX_2_THEME"
: > "$XBOX_2_THEME/.playfusion-generated-xbox-2"
rm -f -- "$XBOX_2_THEME/xbox_background.mp4"
sed -i \
    -e 's/^description = .*/description = "Native-rendered Xbox-inspired PlayFusion 2.0 dashboard. Resolution-independent and optimized for low-power graphics."/' \
    -e 's/^background_selection = .*/background_selection = "Xbox 2.0"/' \
    -e 's/^cursor_color = .*/cursor_color = "YELLOW"/' \
    -e 's/^profile_badge_position = .*/profile_badge_position = "LEFT"/' \
    -e 's/^boot_animation = .*/boot_animation = "boot_animation.mp4"/' \
    "$XBOX_2_THEME/theme.toml"
chown -R gamer:gamer "$XBOX_2_THEME"

test -s "$XBOX_2_THEME/boot_animation.mp4"
cover_count=$(find "$XBOX_2_THEME/system-folders" -maxdepth 1 -type f -name '*.png' | wc -l)
test "$cover_count" -ge 30
grep -q '^background_selection = "Xbox 2.0"$' "$XBOX_2_THEME/theme.toml"
grep -q '^cursor_color = "YELLOW"$' "$XBOX_2_THEME/theme.toml"
grep -q '^profile_badge_position = "LEFT"$' "$XBOX_2_THEME/theme.toml"
grep -q '^boot_animation = "boot_animation.mp4"$' "$XBOX_2_THEME/theme.toml"

# Keep a recoverable copy of the currently working launch path before the
# 1.0.2 UI and optional splash hook are installed.
mkdir -p "$BACKUP_ROOT"
for file in /usr/bin/kazeta-bios /usr/bin/kazeta-session; do
    if test -e "$file" && ! test -e "$BACKUP_ROOT/$(basename "$file")"; then
        cp -a -- "$file" "$BACKUP_ROOT/"
    fi
done

install -m 0755 "$BUILD_ROOT/target/release/kazeta-bios" /usr/bin/kazeta-bios
install -m 0755 /home/gamer/pf-stage-rootfs-usr-bin-playfusion-theme-splash \
    /usr/bin/playfusion-theme-splash
install -m 0755 /home/gamer/pf-stage-rootfs-usr-bin-kazeta-session \
    /usr/bin/kazeta-session

bash -n /usr/bin/playfusion-theme-splash
bash -n /usr/bin/kazeta-session
ldd /usr/bin/kazeta-bios >/dev/null

printf 'Xbox 2.0 system folders: %s\n' "$cover_count"
sha256sum /usr/bin/kazeta-bios /usr/bin/playfusion-theme-splash /usr/bin/kazeta-session
