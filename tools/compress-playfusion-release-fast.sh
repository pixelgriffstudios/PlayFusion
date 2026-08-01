#!/usr/bin/env sh
set -eu
OUT=/var/tmp/playfusion-release-output
tar -C "$OUT" -c -I 'xz -T0 -0' \
    -f "$OUT/playfusion-1.0-public.img.tar.xz" \
    playfusion-1.0-public.img
sha256sum "$OUT/playfusion-1.0-public.img.tar.xz" > "$OUT/sha256sum.txt"
echo done > /var/tmp/playfusion-fast-compress.exit
