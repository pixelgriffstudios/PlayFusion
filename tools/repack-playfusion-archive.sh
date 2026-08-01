#!/usr/bin/env bash
set -euo pipefail

OUTPUT=/var/tmp/playfusion-release-output
STREAM="$OUTPUT/kazeta-2025-0_545b900.img"
ARCHIVE="$OUTPUT/kazeta-2025-0_545b900.img.tar.xz"

test "$(id -u)" -eq 0
test -f "$STREAM"
rm -f -- "$ARCHIVE" "$OUTPUT/sha256sum.txt"

# Use explicit XZ environment options instead of embedding options in tar's
# compressor string.  This avoids argument-splitting differences in live media.
XZ_OPT='-T0 -1' tar -C "$OUTPUT" -cJf "$ARCHIVE" "$(basename "$STREAM")"
sha256sum "$ARCHIVE" > "$OUTPUT/sha256sum.txt"
rm -f -- "$STREAM"

echo CLEAN_REPACK_OK
du -sh "$ARCHIVE"
cat "$OUTPUT/sha256sum.txt"
