#!/usr/bin/env bash
set -euo pipefail

BUILD=/var/tmp/pf-release-v103
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
    "$PAYLOAD/etc/playfusion-update-public.pem" \
    "$PAYLOAD/etc/systemd/system/playfusion-update-health.service"
printf 'PRODUCT=PlayFusion\nVERSION=1.0.3\n' > "$PAYLOAD/etc/playfusion-release"

while IFS= read -r -d '' file; do
    if test "$(head -c 2 "$file")" = '#!'; then chmod 0755 "$file"; fi
done < <(find "$PAYLOAD" -type f -print0)
chown -R root:root "$PAYLOAD"

"$SOURCE/tools/verify-public-rootfs.sh" "$PAYLOAD"
bash -n "$SOURCE/tools/legacy-update-bridge/post-install-v1.0.3.sh"
chmod 0755 "$SOURCE/tools/build-playfusion-update.sh"
"$SOURCE/tools/build-playfusion-update.sh" \
    "$PAYLOAD" 1.0.3 1.0.0 "$BUILD/private.pem" "$OUT" \
    "$SOURCE/tools/legacy-update-bridge/post-install-v1.0.3.sh"

KIT="$OUT/PlayFusion-legacy-update-v1.0.3"
mkdir -p "$KIT"
for executable in upgrade-to-plus.sh; do
    install -m 0755 "$SOURCE/tools/legacy-update-bridge/$executable" "$KIT/$executable"
done
install -m 0755 "$SOURCE/rootfs/usr/bin/playfusion-update-helper" "$KIT/playfusion-update-helper"
install -m 0755 "$SOURCE/rootfs/usr/bin/playfusion-update-health" "$KIT/playfusion-update-health"
install -m 0644 "$SOURCE/rootfs/etc/systemd/system/playfusion-update-health.service" "$KIT/playfusion-update-health.service"
install -m 0440 "$SOURCE/rootfs/etc/sudoers.d/playfusion-update" "$KIT/playfusion-update.sudoers"
install -m 0644 "$SOURCE/rootfs/etc/playfusion-update-public.pem" "$KIT/playfusion-update-public.pem"
cp "$OUT/PlayFusion-update-v1.0.3.pfu" \
    "$OUT/PlayFusion-update-v1.0.3.pfu.sha256" \
    "$OUT/PlayFusion-update-v1.0.3.pfu.sig" "$KIT/"

cd "$OUT"
python3 - <<'PY'
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile
root = Path("PlayFusion-legacy-update-v1.0.3")
with ZipFile("PlayFusion-legacy-update-v1.0.3.zip", "w", ZIP_DEFLATED,
             compresslevel=6, allowZip64=True) as archive:
    for path in sorted(root.rglob("*")):
        if path.is_file(): archive.write(path, path.as_posix())
PY
sha256sum PlayFusion-update-v1.0.3.pfu \
    PlayFusion-update-v1.0.3.pfu.sha256 \
    PlayFusion-update-v1.0.3.pfu.sig \
    PlayFusion-legacy-update-v1.0.3.zip > SHA256SUMS-v1.0.3.txt
chmod 0644 PlayFusion-update-v1.0.3.pfu* \
    PlayFusion-legacy-update-v1.0.3.zip SHA256SUMS-v1.0.3.txt
"$SOURCE/tools/verify-public-rootfs.sh" "$PAYLOAD"
ls -lh PlayFusion-update-v1.0.3.pfu* \
    PlayFusion-legacy-update-v1.0.3.zip SHA256SUMS-v1.0.3.txt
