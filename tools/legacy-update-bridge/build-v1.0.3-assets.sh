#!/usr/bin/env bash
set -euo pipefail

VERSION=${PLAYFUSION_VERSION:-1.0.3}
MINIMUM_VERSION=${PLAYFUSION_MINIMUM_VERSION:-1.0.0}
VERSION_KEY=${VERSION//./}
BUILD="/var/tmp/pf-release-v${VERSION_KEY}"
SOURCE="$BUILD/source"
PAYLOAD="$BUILD/payload"
OUT="$BUILD/out"
LIVE_ROOT=/frzr_root/deployments/playfusion-1.0-public

test "$(id -u)" -eq 0
rm -rf -- "$BUILD"
mkdir -p "$SOURCE" "$PAYLOAD" "$OUT"
tar -xzf /tmp/playfusion-v1.0.3-source.tar.gz -C "$SOURCE"
find "$SOURCE/tools" -type f -name '*.sh' -exec chmod 0755 {} +
install -o root -g root -m 0600 /tmp/playfusion-update-private.pem "$BUILD/private.pem"

cp -a -- "$SOURCE/rootfs/." "$PAYLOAD/"

# A release may be staged from a Windows checkout.  Git's Windows defaults can
# silently turn Linux scripts, sudoers rules, systemd units and configuration
# files into CRLF text.  A CR at the end of a shebang makes Linux look for
# "bash\r" and the launcher exits with status 127.  Normalize UTF-8 text before
# setting modes or signing the update; binary assets are left byte-for-byte
# unchanged.
python3 - "$PAYLOAD" <<'PY'
from pathlib import Path
import sys

root = Path(sys.argv[1])
changed = 0
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
    normalized = data.replace(b"\r\n", b"\n").replace(b"\r", b"\n")
    if normalized != data:
        path.write_bytes(normalized)
        changed += 1
print(f"Normalized LF line endings in {changed} payload files.")
PY

install -D -o root -g root -m 0755 "$LIVE_ROOT/usr/bin/kazeta-bios" \
    "$PAYLOAD/usr/bin/kazeta-bios"
install -D -o root -g root -m 0755 \
    "$LIVE_ROOT/usr/local/lib/libplayfusion_projectm_native.so" \
    "$PAYLOAD/usr/local/lib/libplayfusion_projectm_native.so"
mkdir -p "$PAYLOAD/opt/playfusion"
cp -a -- "$LIVE_ROOT/opt/playfusion/projectm-4.1.6" "$PAYLOAD/opt/playfusion/"

# Runtimes, user data, firmware and keys are never part of an incremental OS
# update. Existing user settings live under /var/kazeta and are untouched.
find "$PAYLOAD" -type f -name '*.kzr' -delete
# The native updater deliberately refuses to replace its own trust and health
# components while it is running. Older releases receive these four files from
# the verified legacy bridge before the signed payload is applied.
rm -f -- \
    "$PAYLOAD/usr/bin/playfusion-update-helper" \
    "$PAYLOAD/usr/bin/playfusion-update-health" \
    "$PAYLOAD/usr/bin/playfusion-game-exit-hotkey" \
    "$PAYLOAD/etc/playfusion-update-public.pem" \
    "$PAYLOAD/etc/systemd/system/playfusion-update-health.service"
printf 'PRODUCT=PlayFusion\nVERSION=%s\n' "$VERSION" > "$PAYLOAD/etc/playfusion-release"

while IFS= read -r -d '' file; do
    if test "$(head -c 2 "$file")" = '#!'; then chmod 0755 "$file"; fi
done < <(find "$PAYLOAD" -type f -print0)
chown -R root:root "$PAYLOAD"

"$SOURCE/tools/verify-public-rootfs.sh" "$PAYLOAD"
bash -n "$SOURCE/tools/legacy-update-bridge/post-install-v1.0.3.sh"
chmod 0755 "$SOURCE/tools/build-playfusion-update.sh"
"$SOURCE/tools/build-playfusion-update.sh" \
    "$PAYLOAD" "$VERSION" "$MINIMUM_VERSION" "$BUILD/private.pem" "$OUT" \
    "$SOURCE/tools/legacy-update-bridge/post-install-v1.0.3.sh"

KIT="$OUT/PlayFusion-legacy-update-v${VERSION}"
mkdir -p "$KIT"
for executable in upgrade-to-plus.sh; do
    install -m 0755 "$SOURCE/tools/legacy-update-bridge/$executable" "$KIT/$executable"
done
sed -i "s/^VERSION=\"1\.0\.3\"$/VERSION=\"${VERSION}\"/" "$KIT/upgrade-to-plus.sh"
install -m 0755 "$SOURCE/rootfs/usr/bin/playfusion-update-helper" "$KIT/playfusion-update-helper"
install -m 0755 "$SOURCE/rootfs/usr/bin/playfusion-update-health" "$KIT/playfusion-update-health"
install -m 0755 "$SOURCE/rootfs/usr/bin/playfusion-game-exit-hotkey" "$KIT/playfusion-game-exit-hotkey"
install -m 0644 "$SOURCE/rootfs/etc/systemd/system/playfusion-update-health.service" "$KIT/playfusion-update-health.service"
install -m 0440 "$SOURCE/rootfs/etc/sudoers.d/playfusion-update" "$KIT/playfusion-update.sudoers"
install -m 0644 "$SOURCE/rootfs/etc/playfusion-update-public.pem" "$KIT/playfusion-update-public.pem"
cp "$OUT/PlayFusion-update-v${VERSION}.pfu" \
    "$OUT/PlayFusion-update-v${VERSION}.pfu.sha256" \
    "$OUT/PlayFusion-update-v${VERSION}.pfu.sig" "$KIT/"

cd "$OUT"
python3 - "$VERSION" <<'PY'
from pathlib import Path
import sys
from zipfile import ZIP_DEFLATED, ZipFile
version = sys.argv[1]
root = Path(f"PlayFusion-legacy-update-v{version}")
with ZipFile(f"PlayFusion-legacy-update-v{version}.zip", "w", ZIP_DEFLATED,
             compresslevel=6, allowZip64=True) as archive:
    for path in sorted(root.rglob("*")):
        if path.is_file(): archive.write(path, path.as_posix())
PY
sha256sum "PlayFusion-update-v${VERSION}.pfu" \
    "PlayFusion-update-v${VERSION}.pfu.sha256" \
    "PlayFusion-update-v${VERSION}.pfu.sig" \
    "PlayFusion-legacy-update-v${VERSION}.zip" > "SHA256SUMS-v${VERSION}.txt"
chmod 0644 "PlayFusion-update-v${VERSION}.pfu"* \
    "PlayFusion-legacy-update-v${VERSION}.zip" "SHA256SUMS-v${VERSION}.txt"
"$SOURCE/tools/verify-public-rootfs.sh" "$PAYLOAD"
ls -lh "PlayFusion-update-v${VERSION}.pfu"* \
    "PlayFusion-legacy-update-v${VERSION}.zip" "SHA256SUMS-v${VERSION}.txt"
