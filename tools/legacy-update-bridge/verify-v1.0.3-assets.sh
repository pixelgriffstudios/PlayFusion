#!/usr/bin/env bash
set -Eeuo pipefail

OUT=${1:-/var/tmp/pf-release-v103/out}
SOURCE=${2:-/var/tmp/pf-release-v103/source}
PUBLIC_KEY=${3:-/etc/playfusion-update-public.pem}
VERSION=1.0.3
PACKAGE="$OUT/PlayFusion-update-v${VERSION}.pfu"
CHECKSUM="${PACKAGE}.sha256"
SIGNATURE="${PACKAGE}.sig"
LEGACY="$OUT/PlayFusion-legacy-update-v${VERSION}.zip"

for path in "$PACKAGE" "$CHECKSUM" "$SIGNATURE" "$LEGACY" "$PUBLIC_KEY"; do
    test -s "$path" || { printf 'Missing release asset: %s\n' "$path" >&2; exit 1; }
done

work=$(mktemp -d /var/tmp/playfusion-v103-verify.XXXXXX)
trap 'rm -rf -- "$work"' EXIT

cd "$OUT"
sha256sum -c "$(basename -- "$CHECKSUM")"
openssl dgst -sha256 -verify "$PUBLIC_KEY" -signature "$SIGNATURE" "$PACKAGE"

while IFS= read -r entry; do
    entry=${entry#./}
    case "$entry" in
        manifest.toml|payload.tar.zst|delete.list|post-install|'') ;;
        *) printf 'Unexpected signed package member: %s\n' "$entry" >&2; exit 1 ;;
    esac
done < <(bsdtar -tf "$PACKAGE")

bsdtar -xf "$PACKAGE" -C "$work"
python3 - "$work/manifest.toml" <<'PY'
import sys, tomllib
with open(sys.argv[1], "rb") as handle:
    data = tomllib.load(handle)
expected = {
    "product": "PlayFusion",
    "format": 1,
    "version": "1.0.3",
    "minimum_version": "1.0.0",
    "restart": "reboot",
}
for key, value in expected.items():
    if data.get(key) != value:
        raise SystemExit(f"bad manifest {key}: {data.get(key)!r}")
PY

test "$(sha256sum "$work/payload.tar.zst" | awk '{print $1}')" = \
    "$(python3 - "$work/manifest.toml" <<'PY'
import sys, tomllib
with open(sys.argv[1], "rb") as handle: print(tomllib.load(handle)["payload_sha256"])
PY
)"
mkdir "$work/root"
bsdtar -xf "$work/payload.tar.zst" -C "$work/root"
"$SOURCE/tools/verify-public-rootfs.sh" "$work/root"
test "$(find "$work/root" -type f -name '*.kzr' -print -quit)" = ''
test ! -e "$work/root/var/kazeta"
test -f "$work/post-install"
bash -n "$work/post-install"

unzip -q "$LEGACY" -d "$work/legacy"
kit="$work/legacy/PlayFusion-legacy-update-v${VERSION}"
for name in \
    "PlayFusion-update-v${VERSION}.pfu" \
    "PlayFusion-update-v${VERSION}.pfu.sha256" \
    "PlayFusion-update-v${VERSION}.pfu.sig" \
    playfusion-update-helper playfusion-update-health \
    playfusion-update-health.service playfusion-update-public.pem \
    playfusion-update.sudoers upgrade-to-plus.sh; do
    test -s "$kit/$name" || { printf 'Missing legacy member: %s\n' "$name" >&2; exit 1; }
done
grep -Fx 'VERSION="1.0.3"' "$kit/upgrade-to-plus.sh" >/dev/null
bash -n "$kit/upgrade-to-plus.sh"
cmp -s "$PACKAGE" "$kit/PlayFusion-update-v${VERSION}.pfu"
cmp -s "$CHECKSUM" "$kit/PlayFusion-update-v${VERSION}.pfu.sha256"
cmp -s "$SIGNATURE" "$kit/PlayFusion-update-v${VERSION}.pfu.sig"
openssl dgst -sha256 -verify "$kit/playfusion-update-public.pem" \
    -signature "$kit/PlayFusion-update-v${VERSION}.pfu.sig" \
    "$kit/PlayFusion-update-v${VERSION}.pfu" >/dev/null

printf 'PlayFusion %s signed and legacy update assets verified.\n' "$VERSION"
