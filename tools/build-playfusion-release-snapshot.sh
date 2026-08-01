#!/usr/bin/env bash
set -euo pipefail

# Capture the known-good PlayFusion deployment without altering its low-level
# Kazeta release identity.  The UI remains PlayFusion-branded, while the boot
# and session layer stays byte-for-byte compatible with the working console.

SOURCE_SUBVOL="/frzr_root/deployments/kazeta-2025-0_545b900"
BUILD_ROOT="/frzr_root/playfusion-release-build"
SNAP_NAME="kazeta-2025-0_545b900"
SNAPSHOT="${BUILD_ROOT}/${SNAP_NAME}"
OUTPUT_DIR="/var/tmp/playfusion-release-output"
STREAM="${OUTPUT_DIR}/${SNAP_NAME}.img"
ARCHIVE="${OUTPUT_DIR}/${SNAP_NAME}.img.tar.xz"
SEED="${OUTPUT_DIR}/playfusion-seed"

test "$(id -u)" -eq 0
test -d "${SOURCE_SUBVOL}"
test "$(findmnt -n -o FSTYPE /frzr_root)" = "btrfs"
test "$(realpath -m "${BUILD_ROOT}")" = "/frzr_root/playfusion-release-build"
test "$(realpath -m "${OUTPUT_DIR}")" = "/var/tmp/playfusion-release-output"

mkdir -p "${BUILD_ROOT}" "${OUTPUT_DIR}"

if btrfs subvolume show "${SNAPSHOT}" >/dev/null 2>&1; then
    btrfs property set -ts "${SNAPSHOT}" ro false || true
    btrfs subvolume delete "${SNAPSHOT}"
elif test -e "${SNAPSHOT}"; then
    echo "Refusing to replace non-subvolume path: ${SNAPSHOT}" >&2
    exit 1
fi

rm -f -- "${STREAM}" "${ARCHIVE}" "${OUTPUT_DIR}/sha256sum.txt"
rm -rf -- "${SEED}"

echo "[1/6] Snapshotting the proven working deployment..."
btrfs subvolume snapshot "${SOURCE_SUBVOL}" "${SNAPSHOT}"

echo "[2/6] Removing private and machine-specific state..."
HOME_DIR="${SNAPSHOT}/home/gamer"
rm -rf -- "${HOME_DIR}/.ssh" "${SNAPSHOT}/root/.ssh"
rm -f -- "${HOME_DIR}/.bash_history" "${HOME_DIR}/.xsession-errors" \
    "${HOME_DIR}/.xsession-errors.old"
rm -rf -- "${HOME_DIR}/.cargo" "${HOME_DIR}/.rustup"

# Keep the exact working UI configuration and home layout.  Only discard old
# backup copies; changing the session's expected files caused the prior image
# to exit on real hardware.
find "${HOME_DIR}/.local/share/kazeta-plus" -maxdepth 1 -type f \
    -name 'config.toml.*' -delete 2>/dev/null || true

# Generate fresh identities on the installed machine and never publish Wi-Fi
# credentials or SSH host keys from the development console.
: > "${SNAPSHOT}/etc/machine-id"
rm -f -- "${SNAPSHOT}"/etc/ssh/ssh_host_*
rm -f -- "${SNAPSHOT}"/etc/NetworkManager/system-connections/* 2>/dev/null || true
chown -R 1000:1000 "${HOME_DIR}"

echo "[3/6] Building a metadata-preserving factory data seed..."
mkdir -p "${SEED}"
cp -a --reflink=auto /var/kazeta/. "${SEED}/"
chown --reference=/var/kazeta "${SEED}"
chmod --reference=/var/kazeta "${SEED}"
touch --reference=/var/kazeta "${SEED}"

# Keep only the approved public games.
find "${SEED}/internal-games" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +
mkdir -p "${SEED}/internal-games/Arcade" "${SEED}/internal-games/PC Games"
cp -a /var/kazeta/internal-games/Arcade/internal-playfusion-arcade \
    "${SEED}/internal-games/Arcade/"
cp -a "/var/kazeta/internal-games/PC Games/internal-hell-on-rails" \
    "${SEED}/internal-games/PC Games/"

# Keep only the approved public album and no personal movies.
find "${SEED}/music" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +
cp -a "/var/kazeta/music/30 years" "${SEED}/music/"
find "${SEED}/movies" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +

# Preserve every working directory and its ownership/mode, but empty personal
# saves, cached media, temporary mounts, and prior upgrade backups.
for clean_dir in cache media-cache loose-rom-cache optical-cache run storage \
    update-backups upgrade-backups saves; do
    mkdir -p "${SEED}/${clean_dir}"
    find "${SEED}/${clean_dir}" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +
done
mkdir -p "${SEED}/saves/default" "${SEED}/saves/profiles"

# Retain the tested Hell on Rails controller profile only.
find "${SEED}/controller-profiles" -mindepth 1 -maxdepth 1 \
    ! -name 'internal-hell-on-rails' -exec rm -rf -- {} +

# Reset profiles while preserving the known-good directory metadata.
find "${SEED}/profiles" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +
cat > "${SEED}/profiles/default.toml" <<'EOF'
id = "default"
name = "Default"
avatar = 0
EOF
printf '%s\n' default > "${SEED}/active-profile"

rm -f -- "${SEED}/session.log" "${SEED}/session.log.old" \
    "${SEED}/state"/*.log "${SEED}/state/.LAUNCH_CMD" \
    "${SEED}/state/.RESTART_SESSION_SENTINEL"
rm -rf -- "${SEED}/state/wireplumber"
: > "${SEED}/state/playfusion-favorites"

chown 1000:1000 "${SEED}/active-profile" "${SEED}/profiles/default.toml" \
    "${SEED}/state/playfusion-favorites"
chmod 0644 "${SEED}/active-profile" "${SEED}/profiles/default.toml" \
    "${SEED}/state/playfusion-favorites"

echo "[4/6] Verifying release contents and boot identity..."
test "$(find "${SNAPSHOT}/usr/share/kazeta/runtimes" -maxdepth 1 -type f -name '*.kzr' | wc -l)" -eq 38
test -f "${SEED}/internal-games/Arcade/internal-playfusion-arcade/cart.kzi"
test -f "${SEED}/internal-games/PC Games/internal-hell-on-rails/cart.kzi"
test -d "${SEED}/music/30 years"
test -n "$(find "${SEED}/firmware" -type f -print -quit)"
test "$(find "${SEED}/internal-games" -mindepth 2 -maxdepth 2 -type d | wc -l)" -eq 2
test "$(head -n 1 "${SNAPSHOT}/build_info")" = "${SNAP_NAME}"

echo "[5/6] Creating Btrfs deployment stream..."
btrfs property set -ts "${SNAPSHOT}" ro true
btrfs send -f "${STREAM}" "${SNAPSHOT}"

echo "[6/6] Compressing deployment archive..."
tar -C "${OUTPUT_DIR}" -c -I 'xz -T0 -1' \
    -f "${ARCHIVE}" "$(basename "${STREAM}")"
sha256sum "${ARCHIVE}" > "${OUTPUT_DIR}/sha256sum.txt"
rm -f -- "${STREAM}"

echo "SNAPSHOT_ARCHIVE=${ARCHIVE}"
echo "SEED_DIR=${SEED}"
du -sh "${ARCHIVE}" "${SEED}"
cat "${OUTPUT_DIR}/sha256sum.txt"
