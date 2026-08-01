#!/usr/bin/env bash
set -euo pipefail

overlay=/tmp/playfusion-live-public-overlay.tar.gz
packages=/tmp/playfusion-live-package-cache.tar
list=/tmp/playfusion-live-public-files.list

cd /

{
    printf '%s\n' \
        usr/bin/kazeta-bios \
        etc/keyd/default.conf \
        etc/vsftpd-kazeta-internal.conf \
        usr/lib/sysusers.d/kazeta-internal-games.conf \
        usr/share/wayland-sessions/kazeta.desktop

    find usr/bin -maxdepth 1 -type f \
        \( -name 'kazeta-*' -o -name 'playfusion-*' \) -print
    find usr/local/bin -maxdepth 1 -type f -print
    find usr/share/playfusion -type f -print
    find usr/share/kazeta/optical-icons -type f -print 2>/dev/null || true
    find etc/super-kazeta -type f -print
    find etc/systemd/system -maxdepth 1 -type f \
        \( -name 'kazeta-*' -o -name 'playfusion-*' -o -name 'super-kazeta-*' \
           -o -name 'optical-*' \) -print
    find etc/sudoers.d -maxdepth 1 -type f \
        \( -name '99-kazeta*' -o -name '99-playfusion*' \
           -o -name '99-super-kazeta*' \) -print
    find etc/udev/rules.d -maxdepth 1 -type f \
        \( -name '*kazeta*' -o -name '*playfusion*' -o -name '*optical*' \) -print
    find etc/lightdm/lightdm.conf.d -maxdepth 1 -type f -print
    find usr/share/inputplumber/profiles -type f -print
} | sort -u | while IFS= read -r path; do
    [[ -e "$path" ]] && printf '%s\n' "$path"
done > "$list"

tar -czf "$overlay" -T "$list"
tar -cf "$packages" \
    --exclude='*.sig' \
    -C /var/cache/pacman/pkg \
    .

chmod 0644 "$overlay" "$packages" "$list"
sha256sum "$overlay" "$packages"
du -h "$overlay" "$packages"
