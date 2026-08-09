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

# Keep this path policy synchronized with playfusion-update-helper so a release
# cannot pass packaging checks and then be rejected by an installed console.
safe_payload_path() {
    local path=${1#./}
    path=${path%/}
    case "$path" in
        ''|/*|.|..|var/kazeta/*|home/*|root/*|boot/*|dev/*|proc/*|sys/*|run/*)
            return 1 ;;
        usr|etc|opt|usr/*|etc/*|opt/*) ;;
        *) return 1 ;;
    esac
    case "/$path/" in *'/../'*|*'//'*) return 1 ;; esac
    case "$path" in
        usr/bin/playfusion-update-helper|usr/bin/playfusion-update-health|etc/playfusion-update-public.pem|etc/systemd/system/playfusion-update-health.service)
            return 1 ;;
    esac
}
while IFS= read -r entry; do
    relative=${entry#./}
    test -z "$relative" && continue
    safe_payload_path "$relative" || {
        printf 'Unsafe updater payload path: %s\n' "$relative" >&2
        exit 1
    }
done < <(bsdtar -tf "$work/payload.tar.zst")

mkdir "$work/root"
bsdtar -xf "$work/payload.tar.zst" -C "$work/root"
"$SOURCE/tools/verify-public-rootfs.sh" "$work/root"

# Reject Windows line endings in Linux payload text.  This is a release-blocking
# check because CRLF shebangs fail as "bash\r: No such file or directory" and
# surface in PlayFusion as launcher status 127.
python3 - "$work/root" <<'PY'
from pathlib import Path
import sys

root = Path(sys.argv[1])
bad = []
for path in root.rglob("*"):
    if not path.is_file() or path.is_symlink():
        continue
    data = path.read_bytes()
    if b"\0" in data:
        continue
    try:
        data.decode("utf-8")
    except UnicodeDecodeError:
        continue
    if b"\r" in data:
        bad.append(path.relative_to(root).as_posix())
if bad:
    raise SystemExit("CR/CRLF text leaked into update payload:\n" + "\n".join(bad))
PY

# Parse every shipped shell launcher, not merely the top-level post-install.
# This catches a malformed shebang body or shell syntax before signing.
while IFS= read -r -d '' candidate; do
    test "$(head -c 2 "$candidate")" = '#!' || continue
    first=$(head -n 1 "$candidate")
    case "$first" in
        '#!'*bash*|'#! /bin/sh'|'#!/bin/sh') bash -n "$candidate" ;;
    esac
done < <(find "$work/root/usr/bin" "$work/root/usr/local/bin" \
    -maxdepth 1 -type f -print0)

if command -v visudo >/dev/null 2>&1; then
    for rules in "$work/root"/etc/sudoers.d/*; do
        test -f "$rules" || continue
        visudo -cf "$rules" >/dev/null
    done
fi

test "$(find "$work/root" -type f -name '*.kzr' -print -quit)" = ''
test ! -e "$work/root/var/kazeta"
for protected in \
    usr/bin/playfusion-update-helper \
    usr/bin/playfusion-update-health \
    usr/bin/playfusion-game-exit-hotkey \
    etc/playfusion-update-public.pem \
    etc/systemd/system/playfusion-update-health.service; do
    test ! -e "$work/root/$protected" || {
        printf 'Protected updater path leaked into payload: %s\n' "$protected" >&2
        exit 1
    }
done
test -f "$work/post-install"
bash -n "$work/post-install"
PLAYFUSION_THEME_TEST_ONLY=1 \
PLAYFUSION_THEME_USER=root \
PLAYFUSION_THEME_GROUP=root \
PLAYFUSION_THEME_TEST_ROOT="$work/theme-install-test" \
PLAYFUSION_THEME_TEST_ARCHIVE_ROOT="$work/root/usr/share/playfusion/themes" \
    bash "$work/post-install"
test -s "$work/theme-install-test/xbox_original/theme.toml"
test -s "$work/theme-install-test/xbox_2_0/theme.toml"

mkdir -p "$work/legacy"
bsdtar -xf "$LEGACY" -C "$work/legacy"
kit="$work/legacy/PlayFusion-legacy-update-v${VERSION}"
for name in \
    "PlayFusion-update-v${VERSION}.pfu" \
    "PlayFusion-update-v${VERSION}.pfu.sha256" \
    "PlayFusion-update-v${VERSION}.pfu.sig" \
    playfusion-update-helper playfusion-update-health \
    playfusion-game-exit-hotkey \
    playfusion-update-health.service playfusion-update-public.pem \
    playfusion-update.sudoers upgrade-to-plus.sh; do
    test -s "$kit/$name" || { printf 'Missing legacy member: %s\n' "$name" >&2; exit 1; }
done
grep -Fx 'VERSION="1.0.3"' "$kit/upgrade-to-plus.sh" >/dev/null
bash -n "$kit/upgrade-to-plus.sh"
test "$(sha256sum "$PACKAGE" | awk '{print $1}')" = \
    "$(sha256sum "$kit/PlayFusion-update-v${VERSION}.pfu" | awk '{print $1}')"
test "$(sha256sum "$CHECKSUM" | awk '{print $1}')" = \
    "$(sha256sum "$kit/PlayFusion-update-v${VERSION}.pfu.sha256" | awk '{print $1}')"
test "$(sha256sum "$SIGNATURE" | awk '{print $1}')" = \
    "$(sha256sum "$kit/PlayFusion-update-v${VERSION}.pfu.sig" | awk '{print $1}')"
openssl dgst -sha256 -verify "$kit/playfusion-update-public.pem" \
    -signature "$kit/PlayFusion-update-v${VERSION}.pfu.sig" \
    "$kit/PlayFusion-update-v${VERSION}.pfu" >/dev/null

printf 'PlayFusion %s signed and legacy update assets verified.\n' "$VERSION"
