#!/usr/bin/env bash
set -euo pipefail

HOME_DATA=/home/gamer/.local/share/kazeta-plus
PERSISTENT_DATA=/var/kazeta/user-data/kazeta-plus
THEME_ROOT="$PERSISTENT_DATA/themes"

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
    install -d -o gamer -g gamer -m 0755 "$THEME_ROOT/$folder"
    cp -a -- "$stage/$folder/." "$THEME_ROOT/$folder/"
    chown -R gamer:gamer "$THEME_ROOT/$folder"
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

chown -R gamer:gamer /var/kazeta/user-data/kazeta-plus \
    /var/kazeta/state/projectm-home /var/kazeta/state/wireplumber

# Merge shipped theme assets without changing the user's current theme,
# background, profile, audio, or other saved configuration.
install_theme_archive \
    /usr/share/playfusion/themes/xbox_original-PlayFusion-optimized.zip xbox_original
install_theme_archive \
    /usr/share/playfusion/themes/xbox_2_0-PlayFusion.zip xbox_2_0

chmod 0440 /etc/sudoers.d/playfusion-update
systemctl daemon-reload
systemctl enable playfusion-update-health.service >/dev/null
udevadm control --reload 2>/dev/null || true
