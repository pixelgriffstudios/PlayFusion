#!/usr/bin/env bash
set -euo pipefail

package=${1:?Usage: diagnose-playfusion-update-paths.sh PACKAGE.pfu}
work=$(mktemp -d /var/tmp/playfusion-update-path-check.XXXXXX)
trap 'rm -rf -- "$work"' EXIT
bsdtar -xf "$package" -C "$work"

safe_relative_path() {
    local path=${1#./}
    path=${path%/}
    case "$path" in
        ''|/*|.|..|var/kazeta/*|home/*|root/*|boot/*|dev/*|proc/*|sys/*|run/*)
            return 1 ;;
        usr|etc|opt|usr/*|etc/*|opt/*)
            ;;
        *) return 1 ;;
    esac
    case "/$path/" in *'/../'*|*'//'*) return 1 ;; esac
    case "$path" in
        usr/bin/playfusion-update-helper|usr/bin/playfusion-update-health|etc/playfusion-update-public.pem|etc/systemd/system/playfusion-update-health.service)
            return 1 ;;
    esac
}

status=0
while IFS= read -r entry; do
    relative=${entry#./}
    test -z "$relative" && continue
    if ! safe_relative_path "$relative"; then
        printf 'UNSAFE %s\n' "$relative"
        status=1
    fi
done < <(bsdtar -tf "$work/payload.tar.zst")
exit "$status"
