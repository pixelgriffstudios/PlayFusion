#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
source_dir="$repo_root/runtimes/waydroid-1.0"
output=${1:-"$repo_root/waydroid-1.0.kzr"}

command -v mkfs.erofs >/dev/null 2>&1 || {
    echo "mkfs.erofs is required (erofs-utils)." >&2
    exit 1
}
[[ -x "$source_dir/.kazeta/share/run" ]] || chmod 0755 "$source_dir/.kazeta/share/run"

rm -f "$output"
mkfs.erofs -zlz4hc,12 "$output" "$source_dir"
fsck.erofs --extract=/tmp/playfusion-waydroid-runtime-check "$output" >/dev/null
test -x /tmp/playfusion-waydroid-runtime-check/.kazeta/share/run
rm -rf /tmp/playfusion-waydroid-runtime-check
sha256sum "$output"
