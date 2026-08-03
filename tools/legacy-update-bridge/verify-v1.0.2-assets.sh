#!/usr/bin/env bash
set -euo pipefail

OUT=${1:-/var/tmp/pf-release-v102/out}
PACKAGE="$OUT/PlayFusion-update-v1.0.2.pfu"
CHECKSUM="$PACKAGE.sha256"
SIGNATURE="$PACKAGE.sig"
ZIP="$OUT/PlayFusion-legacy-update-v1.0.2.zip"
PUBLIC_KEY=/var/tmp/pf-release-v102/source/rootfs/etc/playfusion-update-public.pem
WORK=$(mktemp -d)
trap 'rm -rf -- "$WORK"' EXIT

(cd "$OUT" && sha256sum -c PlayFusion-update-v1.0.2.pfu.sha256)
openssl dgst -sha256 -verify "$PUBLIC_KEY" -signature "$SIGNATURE" "$PACKAGE" >/dev/null

printf '%s\n' manifest.toml payload.tar.zst post-install > "$WORK/expected-members"
bsdtar -tf "$PACKAGE" | sed 's|^\./||' | sort > "$WORK/actual-members"
sort -o "$WORK/expected-members" "$WORK/expected-members"
cmp "$WORK/expected-members" "$WORK/actual-members"
bsdtar -xf "$PACKAGE" -C "$WORK"

payload_hash=$(sed -n 's/^payload_sha256 = "\([0-9a-f]*\)"$/\1/p' "$WORK/manifest.toml")
test "${#payload_hash}" -eq 64
test "$payload_hash" = "$(sha256sum "$WORK/payload.tar.zst" | awk '{ print $1 }')"
bash -n "$WORK/post-install"

mkdir "$WORK/root"
bsdtar -xf "$WORK/payload.tar.zst" -C "$WORK/root"
test -x "$WORK/root/usr/bin/kazeta-bios"
test "$(stat -c %u:%g "$WORK/root/usr/bin/kazeta-bios")" = 0:0
test "$(stat -c %a "$WORK/root/usr/bin/kazeta-bios")" = 755
test ! -e "$WORK/root/var"
test ! -e "$WORK/root/home"
for forbidden in \
    usr/bin/playfusion-update-helper \
    usr/bin/playfusion-update-health \
    etc/playfusion-update-public.pem \
    etc/playfusion-release \
    etc/systemd/system/playfusion-update-health.service \
    etc/sudoers.d/playfusion-update; do
    test ! -e "$WORK/root/$forbidden"
done

mkdir "$WORK/zip"
bsdtar -xf "$ZIP" -C "$WORK/zip"
KIT="$WORK/zip/PlayFusion-legacy-update-v1.0.2"
test -x "$KIT/upgrade-to-plus.sh"
bash -n "$KIT/upgrade-to-plus.sh"
cmp "$PACKAGE" "$KIT/PlayFusion-update-v1.0.2.pfu"
cmp "$CHECKSUM" "$KIT/PlayFusion-update-v1.0.2.pfu.sha256"
cmp "$SIGNATURE" "$KIT/PlayFusion-update-v1.0.2.pfu.sig"
openssl dgst -sha256 -verify "$KIT/playfusion-update-public.pem" \
    -signature "$KIT/PlayFusion-update-v1.0.2.pfu.sig" \
    "$KIT/PlayFusion-update-v1.0.2.pfu" >/dev/null

if bsdtar -tf "$ZIP" | grep -Ei '(private|private\.pem|signing)' >/dev/null; then
    echo 'Private signing material leaked into the legacy ZIP.' >&2
    exit 1
fi

printf 'Verified signed PlayFusion 1.0.2 PFU and legacy ZIP.\n'
