#!/usr/bin/env bash
set -euo pipefail

for package in "$@"; do
    archive=$(find /var/cache/pacman/pkg -maxdepth 1 -type f -name "${package}-*.pkg.tar.*" \
        ! -name '*.sig' -printf '%T@\t%p\n' | sort -nr | head -n 1 | cut -f2-)
    if [[ -z "$archive" ]]; then
        printf '%s\tMISSING_ARCHIVE\n' "$package"
        continue
    fi

    conflicts=0
    while IFS= read -r entry; do
        entry=${entry#./}
        [[ -n "$entry" && "$entry" != */ ]] || continue
        path="/$entry"
        [[ -e "$path" || -L "$path" ]] || continue
        if ! pacman -Qo "$path" >/dev/null 2>&1; then
            ((conflicts += 1))
            if (( conflicts <= 12 )); then
                printf '%s\tUNOWNED\t%s\n' "$package" "$path"
            fi
        fi
    done < <(bsdtar -tf "$archive")
    printf '%s\tUNOWNED_COUNT\t%d\n' "$package" "$conflicts"
done
