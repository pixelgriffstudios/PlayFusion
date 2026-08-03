#!/usr/bin/env bash
set -euo pipefail

HOME_DATA=/home/gamer/.local/share/kazeta-plus
PERSISTENT_DATA=/var/kazeta/user-data/kazeta-plus
THEME_ROOT="$PERSISTENT_DATA/themes"

install_theme_archive() {
    local archive=$1
    local folder=$2
    local stage entry

    test -s "$archive"
    stage=$(mktemp -d "/var/tmp/playfusion-theme-${folder}.XXXXXX")
    while IFS= read -r entry; do
        entry=${entry#./}
        case "$entry" in
            "$folder"|"$folder/"|"$folder/"*) ;;
            *) rm -rf -- "$stage"; return 1 ;;
        esac
        case "/$entry/" in
            *'/../'*|*'//'*) rm -rf -- "$stage"; return 1 ;;
        esac
    done < <(bsdtar -tf "$archive")
    bsdtar --no-same-owner -xf "$archive" -C "$stage"
    test -s "$stage/$folder/theme.toml"
    install -d -o gamer -g gamer -m 0755 "$THEME_ROOT/$folder"
    cp -a -- "$stage/$folder/." "$THEME_ROOT/$folder/"
    chown -R gamer:gamer "$THEME_ROOT/$folder"
    rm -rf -- "$stage"
}

install -d -o gamer -g gamer -m 0755 /var/kazeta/user-data /var/kazeta/state
install -d -o gamer -g gamer -m 0755 /var/kazeta/state/projectm-home \
    /var/kazeta/state/wireplumber

if test -d "$HOME_DATA" && ! test -L "$HOME_DATA"; then
    install -d -o gamer -g gamer -m 0755 "$PERSISTENT_DATA"
    cp -a -- "$HOME_DATA/." "$PERSISTENT_DATA/"
    backup="${HOME_DATA}.pre-1.0.2"
    if ! test -e "$backup"; then
        mv -- "$HOME_DATA" "$backup"
    else
        rm -rf -- "$HOME_DATA"
    fi
    ln -s "$PERSISTENT_DATA" "$HOME_DATA"
elif ! test -e "$HOME_DATA"; then
    install -d -o gamer -g gamer -m 0755 "$PERSISTENT_DATA"
    ln -s "$PERSISTENT_DATA" "$HOME_DATA"
fi

chown -R gamer:gamer /var/kazeta/user-data/kazeta-plus \
    /var/kazeta/state/projectm-home /var/kazeta/state/wireplumber

# Ship complete themes instead of assuming they were already present on an
# older Kazeta/PlayFusion installation. Existing theme directories are merged
# so unrelated user-added files remain intact.
install_theme_archive \
    /usr/share/playfusion/themes/xbox_original-PlayFusion-optimized.zip \
    xbox_original
install_theme_archive \
    /usr/share/playfusion/themes/xbox_2_0-PlayFusion.zip \
    xbox_2_0

chmod 0440 /etc/sudoers.d/playfusion-update
systemctl daemon-reload
systemctl enable playfusion-update-health.service >/dev/null
udevadm control --reload 2>/dev/null || true
