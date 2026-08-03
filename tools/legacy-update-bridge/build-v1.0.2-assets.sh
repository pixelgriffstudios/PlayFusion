#!/usr/bin/env bash
set -euo pipefail

BUILD=/var/tmp/pf-release-v102
SOURCE="$BUILD/source"
PAYLOAD="$BUILD/payload"
OUT="$BUILD/out"

test "$(id -u)" -eq 0
mkdir -p "$SOURCE" "$PAYLOAD" "$OUT"
tar -xzf /tmp/rootfs-v1.0.2.tar.gz -C "$SOURCE"
tar -xzf /tmp/legacy-update-bridge-source.tar.gz -C "$SOURCE"
install -o root -g root -m 0600 /tmp/playfusion-update-private.pem "$BUILD/private.pem"

while IFS= read -r relative; do
    test -n "$relative" || continue
    case "$relative" in
        usr/bin/playfusion-update-helper|usr/bin/playfusion-update-health|etc/playfusion-update-public.pem|etc/playfusion-release|etc/systemd/system/playfusion-update-health.service|etc/sudoers.d/playfusion-update)
            continue
            ;;
    esac
    source_file="$SOURCE/rootfs/$relative"
    test -f "$source_file"
    install -D -o root -g root -m 0644 "$source_file" "$PAYLOAD/$relative"
done < "$SOURCE/tools/legacy-update-bridge/payload-files-v1.0.2.txt"

install -D -o root -g root -m 0755 \
    /var/tmp/playfusion-v102-build/bios/target/release/kazeta-bios \
    "$PAYLOAD/usr/bin/kazeta-bios"

while IFS= read -r -d '' file; do
    if test "$(head -c 2 "$file")" = '#!'; then
        chmod 0755 "$file"
    fi
done < <(find "$PAYLOAD" -type f -print0)
chown -R root:root "$PAYLOAD"

bash -n "$SOURCE/tools/legacy-update-bridge/post-install-v1.0.2.sh"
chmod 0755 /tmp/build-playfusion-update.sh
/tmp/build-playfusion-update.sh \
    "$PAYLOAD" 1.0.2 1.0.0 "$BUILD/private.pem" "$OUT" \
    "$SOURCE/tools/legacy-update-bridge/post-install-v1.0.2.sh"

KIT="$OUT/PlayFusion-legacy-update-v1.0.2"
mkdir -p "$KIT"
install -m 0755 "$SOURCE/tools/legacy-update-bridge/upgrade-to-plus.sh" \
    "$KIT/upgrade-to-plus.sh"
install -m 0755 "$SOURCE/rootfs/usr/bin/playfusion-update-helper" \
    "$KIT/playfusion-update-helper"
install -m 0755 "$SOURCE/rootfs/usr/bin/playfusion-update-health" \
    "$KIT/playfusion-update-health"
install -m 0644 "$SOURCE/rootfs/etc/systemd/system/playfusion-update-health.service" \
    "$KIT/playfusion-update-health.service"
install -m 0440 "$SOURCE/rootfs/etc/sudoers.d/playfusion-update" \
    "$KIT/playfusion-update.sudoers"
install -m 0644 "$SOURCE/rootfs/etc/playfusion-update-public.pem" \
    "$KIT/playfusion-update-public.pem"
cp "$OUT/PlayFusion-update-v1.0.2.pfu" \
    "$OUT/PlayFusion-update-v1.0.2.pfu.sha256" \
    "$OUT/PlayFusion-update-v1.0.2.pfu.sig" "$KIT/"

bash -n "$KIT/upgrade-to-plus.sh"
cd "$OUT"
rm -f PlayFusion-legacy-update-v1.0.2.zip SHA256SUMS-v1.0.2.txt
python3 - <<'PY'
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

root = Path("PlayFusion-legacy-update-v1.0.2")
with ZipFile(
    "PlayFusion-legacy-update-v1.0.2.zip",
    "w",
    compression=ZIP_DEFLATED,
    compresslevel=6,
    allowZip64=False,
) as archive:
    for path in sorted(root.rglob("*")):
        if path.is_file():
            archive.write(path, path.as_posix())
PY
sha256sum PlayFusion-update-v1.0.2.pfu \
    PlayFusion-update-v1.0.2.pfu.sha256 \
    PlayFusion-update-v1.0.2.pfu.sig \
    PlayFusion-legacy-update-v1.0.2.zip > SHA256SUMS-v1.0.2.txt
chmod 0644 PlayFusion-update-v1.0.2.pfu \
    PlayFusion-update-v1.0.2.pfu.sha256 \
    PlayFusion-update-v1.0.2.pfu.sig \
    PlayFusion-legacy-update-v1.0.2.zip SHA256SUMS-v1.0.2.txt
ls -lh PlayFusion-update-v1.0.2.pfu* \
    PlayFusion-legacy-update-v1.0.2.zip SHA256SUMS-v1.0.2.txt
