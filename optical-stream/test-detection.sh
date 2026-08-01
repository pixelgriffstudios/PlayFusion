#!/bin/bash
set -Eeuo pipefail

HELPER=${1:-/usr/bin/kazeta-optical-disc-helper}
STREAM_ROOT=${2:-/tmp/kazeta-cdstream-test/mount}
PROBE=${3:-dvd}
FUNCTIONS_FILE=$(mktemp)
trap 'rm -f "$FUNCTIONS_FILE"' EXIT

sed \
    -e '/^case "${1:-}" in/,$d' \
    -e "s#/run/kazeta/cdstream#$STREAM_ROOT#g" \
    "$HELPER" > "$FUNCTIONS_FILE"
# shellcheck disable=SC1090
source "$FUNCTIONS_FILE"

stage_streamed_cart() {
    printf 'DETECTED platform=%s title=%s id=%s runtime=%s kind=%s\n' \
        "$2" "$3" "$4" "$5" "$7"
}

case "$PROBE" in
    dvd) detect_streamed_dvd /dev/sr0 ;;
    cd) detect_streamed_cd /dev/sr0 ;;
    *) printf 'Usage: %s [HELPER] [STREAM_ROOT] [dvd|cd]\n' "$0" >&2; exit 2 ;;
esac
