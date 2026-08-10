#!/usr/bin/env bash
set -euo pipefail

HOME_DATA=/home/gamer/.local/share/kazeta-plus
PERSISTENT_DATA=/var/kazeta/user-data/kazeta-plus
THEME_ROOT="$PERSISTENT_DATA/themes"
THEME_USER=${PLAYFUSION_THEME_USER:-gamer}
THEME_GROUP=${PLAYFUSION_THEME_GROUP:-gamer}

# chown -R dereferences symbolic links on some paths.  ProjectM's persistent
# preset collection intentionally contains links, and a link can be dangling
# after a preset refresh.  Own the links themselves so one stale preset cannot
# abort an otherwise valid OS update.
safe_recursive_chown() {
    local owner=$1 path
    shift
    for path in "$@"; do
        test -e "$path" || test -L "$path" || continue
        chown -hR "$owner" "$path"
    done
}

install_theme_archive() {
    local archive=$1 folder=$2 stage entry normalized
    test -s "$archive"
    stage=$(mktemp -d "/var/tmp/playfusion-theme-${folder}.XXXXXX")
    while IFS= read -r entry; do
        normalized=${entry#./}
        # Archive directory records conventionally end in '/'. Strip that
        # marker before validating so a legitimate "theme/" entry is not
        # mistaken for an embedded double slash.
        normalized=${normalized%/}
        case "$normalized" in "$folder"|"$folder/"*) ;; *) rm -rf -- "$stage"; return 1 ;; esac
        case "/$normalized/" in *'/../'*|*'//'*) rm -rf -- "$stage"; return 1 ;; esac
    done < <(bsdtar -tf "$archive")
    bsdtar --no-same-owner -xf "$archive" -C "$stage"
    test -s "$stage/$folder/theme.toml"
    install -d -o "$THEME_USER" -g "$THEME_GROUP" -m 0755 "$THEME_ROOT/$folder"
    cp -a -- "$stage/$folder/." "$THEME_ROOT/$folder/"
    chown -R "$THEME_USER:$THEME_GROUP" "$THEME_ROOT/$folder"
    rm -rf -- "$stage"
}

# Packaging verification exercises the real archive installer against a
# disposable destination. This catches archive-layout regressions before an
# update is signed without touching a console's persistent data.
if test "${PLAYFUSION_THEME_TEST_ONLY:-0}" = 1; then
    THEME_ROOT=${PLAYFUSION_THEME_TEST_ROOT:?Missing theme test destination}
    test_archives=${PLAYFUSION_THEME_TEST_ARCHIVE_ROOT:?Missing theme test archives}
    install_theme_archive "$test_archives/xbox_original-PlayFusion-optimized.zip" xbox_original
    install_theme_archive "$test_archives/xbox_2_0-PlayFusion.zip" xbox_2_0
    # Reproduce a real console's stale ProjectM preset link.  The release must
    # be rejected if recursive ownership handling cannot tolerate it.
    ownership_fixture="$THEME_ROOT/.ownership-fixture"
    install -d "$ownership_fixture"
    ln -s missing-preset.milk "$ownership_fixture/dangling-preset.milk"
    safe_recursive_chown "$THEME_USER:$THEME_GROUP" "$ownership_fixture"
    test -L "$ownership_fixture/dangling-preset.milk"
    exit 0
fi

install -d -o gamer -g gamer -m 0755 /var/kazeta/user-data /var/kazeta/state
install -d -o gamer -g gamer -m 0755 /var/kazeta/state/projectm-home \
    /var/kazeta/state/wireplumber

if test -d "$HOME_DATA" && ! test -L "$HOME_DATA"; then
    install -d -o gamer -g gamer -m 0755 "$PERSISTENT_DATA"
    cp -a -- "$HOME_DATA/." "$PERSISTENT_DATA/"
    backup="${HOME_DATA}.pre-1.0.3"
    if ! test -e "$backup"; then mv -- "$HOME_DATA" "$backup"; else rm -rf -- "$HOME_DATA"; fi
    ln -s "$PERSISTENT_DATA" "$HOME_DATA"
elif ! test -e "$HOME_DATA"; then
    install -d -o gamer -g gamer -m 0755 "$PERSISTENT_DATA"
    ln -s "$PERSISTENT_DATA" "$HOME_DATA"
fi

safe_recursive_chown gamer:gamer /var/kazeta/user-data/kazeta-plus \
    /var/kazeta/state/projectm-home /var/kazeta/state/wireplumber

# Reassert Linux execution and privilege metadata after the payload is merged.
# Older 1.0.3 assets were prepared from a Windows checkout, which left CRLF in
# several launchers and sudoers rules.  The corrected payload replaces their
# contents; these explicit modes make the repair deterministic on every 1.0.x
# starting point.
while IFS= read -r -d '' file; do
    if test "$(head -c 2 "$file")" = '#!'; then
        chown root:root "$file"
        chmod 0755 "$file"
    fi
done < <(find /usr/bin /usr/local/bin -maxdepth 1 -type f \
    \( -name 'kazeta*' -o -name 'playfusion-*' -o -name 'super-kazeta-*' \) \
    -print0)

for rules in \
    /etc/sudoers.d/99-kazeta-plus \
    /etc/sudoers.d/99-playfusion-media-library \
    /etc/sudoers.d/playfusion-update \
    /etc/sudoers.d/playfusion-waydroid-prepare \
    /etc/sudoers.d/playfusion-waydroid-shell; do
    test -f "$rules" || continue
    chown root:root "$rules"
    chmod 0440 "$rules"
done

# Refresh projectM's private library path and restore all persistent helpers
# that a 1.0.2-to-1.0.3 update expects to be active.
ldconfig
systemctl daemon-reload
for unit in \
    kazeta-import-ftp-runtimes.path \
    kazeta-internal-ftp.service \
    kazeta-normalize-internal-covers.path \
    kazeta-normalize-internal-covers.timer \
    kazeta-profile-loader.service \
    playfusion-loose-rom.path \
    playfusion-storage.service \
    playfusion-update-health.service; do
    systemctl enable "$unit" >/dev/null
done

# Merge shipped theme assets without changing the user's current theme,
# background, profile, audio, or other saved configuration.
install_theme_archive \
    /usr/share/playfusion/themes/xbox_original-PlayFusion-optimized.zip xbox_original
install_theme_archive \
    /usr/share/playfusion/themes/xbox_2_0-PlayFusion.zip xbox_2_0

udevadm control --reload 2>/dev/null || true
