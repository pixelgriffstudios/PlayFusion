#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

# Capture the known-good PlayFusion deployment without altering its low-level
# Kazeta release identity.  The UI remains PlayFusion-branded, while the boot
# and session layer stays byte-for-byte compatible with the working console.

FLAVOR=${PLAYFUSION_FLAVOR:-full}
case "$FLAVOR" in
    full|lite) ;;
    *) echo "PLAYFUSION_FLAVOR must be full or lite" >&2; exit 2 ;;
esac

SOURCE_SUBVOL="/frzr_root/deployments/playfusion-1.0-public"
BUILD_ROOT="/frzr_root/playfusion-release-build-${FLAVOR}"
SNAP_NAME="playfusion-1.0-public"
SNAPSHOT="${BUILD_ROOT}/${SNAP_NAME}"
OUTPUT_DIR="/var/tmp/playfusion-release-output-${FLAVOR}"
STREAM="${OUTPUT_DIR}/${SNAP_NAME}.img"
ARCHIVE="${OUTPUT_DIR}/${SNAP_NAME}.img.tar.xz"
SEED="${OUTPUT_DIR}/playfusion-seed"

test "$(id -u)" -eq 0
test -d "${SOURCE_SUBVOL}"
test "$(findmnt -n -o FSTYPE /frzr_root)" = "btrfs"
test "$(realpath -m "${BUILD_ROOT}")" = "/frzr_root/playfusion-release-build-${FLAVOR}"
test "$(realpath -m "${OUTPUT_DIR}")" = "/var/tmp/playfusion-release-output-${FLAVOR}"

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

# Waydroid's test session data is writable machine state, not part of the
# public operating-system payload.  In particular, Android's package manager
# can apply btrfs.compression attributes to extracted native libraries.  The
# installed PlayFusion root is deliberately mounted nodatacow, where those
# attributes are invalid and cause btrfs receive to abort.  Ship a clean
# per-user Waydroid state directory; the experimental runtime recreates its
# state when it is initialized by a future supported Android cartridge.
rm -rf -- "${HOME_DIR}/.local/share/waydroid"
install -d -o 1000 -g 1000 -m 0755 "${HOME_DIR}/.local/share/waydroid"

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

printf 'PRODUCT=PlayFusion\nVERSION=1.0.3\n' > "${SNAPSHOT}/etc/playfusion-release"
install -d -m 0755 "${SNAPSHOT}/etc/systemd/system/graphical.target.wants"
ln -sfn ../playfusion-update-health.service \
    "${SNAPSHOT}/etc/systemd/system/graphical.target.wants/playfusion-update-health.service"

# Full media bundles every currently installed public runtime, including the
# separately downloaded Windows 1.2 runtime. Lite media intentionally keeps
# only none.kzr, the tiny no-runtime sentinel used by the launcher.
RUNTIME_DIR="${SNAPSHOT}/usr/share/kazeta/runtimes"
mkdir -p "${RUNTIME_DIR}"
if [[ "$FLAVOR" == full ]]; then
    find /var/kazeta/runtimes -maxdepth 1 -type f -name '*.kzr' \
        -exec install -m 0644 {} "${RUNTIME_DIR}/" \; 2>/dev/null || true
else
    find "${RUNTIME_DIR}" -maxdepth 1 -type f -name '*.kzr' \
        ! -name none.kzr -delete
fi

echo "[3/6] Building a metadata-preserving factory data seed..."
mkdir -p "${SEED}"
cp -a --reflink=auto /var/kazeta/. "${SEED}/"
chown --reference=/var/kazeta "${SEED}"
chmod --reference=/var/kazeta "${SEED}"
touch --reference=/var/kazeta "${SEED}"

# Firmware and decryption keys on the development console are private input
# supplied by the owner.  Keep the directory layout so BIOS Manager can import
# authorized files after installation, but never publish the files themselves.
mkdir -p "${SEED}/firmware"
find "${SEED}/firmware" -mindepth 1 -delete
mkdir -p "${SEED}/runtimes"
find "${SEED}/runtimes" -mindepth 1 -delete

# Ship the two approved Xbox themes, but do not publish downloaded test themes
# or the developer console's current selection. The factory UI remains the
# native-resolution ProjectM Fusion and users may opt into other themes.
THEME_ROOT="${SEED}/user-data/kazeta-plus/themes"
if test -d "${THEME_ROOT}"; then
    find "${THEME_ROOT}" -mindepth 1 -maxdepth 1 -type d \
        ! -name xbox_original ! -name xbox_2_0 -exec rm -rf -- {} +
fi
CONFIG="${SEED}/user-data/kazeta-plus/config.toml"
test -f "${CONFIG}"
sed -i \
    -e 's/^resolution = .*/resolution = "1280x720"/' \
    -e 's/^screensaver_idle_seconds = .*/screensaver_idle_seconds = 30/' \
    -e 's/^audio_output = .*/audio_output = "Auto"/' \
    -e 's/^theme = .*/theme = "Default"/' \
    -e 's/^profile_badge_position = .*/profile_badge_position = "RIGHT"/' \
    -e 's/^background_selection = .*/background_selection = "ProjectM Fusion"/' \
    "${CONFIG}"

# Keep only the approved public games.
find "${SEED}/internal-games" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +
mkdir -p "${SEED}/internal-games/Arcade" "${SEED}/internal-games/PC Games"
cp -a /var/kazeta/internal-games/Arcade/internal-playfusion-arcade \
    "${SEED}/internal-games/Arcade/"
cp -a "/var/kazeta/internal-games/PC Games/internal-hell-on-rails" \
    "${SEED}/internal-games/PC Games/"

# No approved public game may smuggle firmware or keys inside its cart folder.
find "${SEED}/internal-games" -type d -iname bios -prune \
    -exec find {} -mindepth 1 -delete \;

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

# The development console's persistent state contains compiler trees, runtime
# inspections, APK staging, rollback binaries, and diagnostics. None belongs
# in a public image. Preserve only projectM's writable preset/font state, which
# is required by the corrected Digital Jukebox.
PROJECTM_FACTORY="${OUTPUT_DIR}/projectm-home-factory"
rm -rf -- "${PROJECTM_FACTORY}"
if test -d "${SEED}/state/projectm-home"; then
    cp -a -- "${SEED}/state/projectm-home" "${PROJECTM_FACTORY}"
fi
find "${SEED}/state" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +
mkdir -p "${SEED}/state/wireplumber"
if test -d "${PROJECTM_FACTORY}"; then
    mv -- "${PROJECTM_FACTORY}" "${SEED}/state/projectm-home"
fi
: > "${SEED}/state/playfusion-favorites"

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

rm -f -- "${SEED}/session.log" "${SEED}/session.log.old"

chown 1000:1000 "${SEED}/active-profile" "${SEED}/profiles/default.toml" \
    "${SEED}/state/playfusion-favorites"
chmod 0644 "${SEED}/active-profile" "${SEED}/profiles/default.toml" \
    "${SEED}/state/playfusion-favorites"

echo "[4/6] Verifying release contents and boot identity..."
if [[ "$FLAVOR" == full ]]; then
    test "$(find "${RUNTIME_DIR}" -maxdepth 1 -type f -name '*.kzr' | wc -l)" -eq 39
    test -s "${RUNTIME_DIR}/windows-1.2-experimental.kzr"
else
    test "$(find "${RUNTIME_DIR}" -maxdepth 1 -type f -name '*.kzr' | wc -l)" -eq 1
    test -s "${RUNTIME_DIR}/none.kzr"
fi
test -f "${SEED}/internal-games/Arcade/internal-playfusion-arcade/cart.kzi"
test -f "${SEED}/internal-games/PC Games/internal-hell-on-rails/cart.kzi"
test -d "${SEED}/music/30 years"
for theme in xbox_original xbox_2_0; do
    test -s "${THEME_ROOT}/${theme}/theme.toml"
    test -s "${THEME_ROOT}/${theme}/boot_animation.mp4"
    test "$(find "${THEME_ROOT}/${theme}/system-folders" -maxdepth 1 -type f -name '*.png' | wc -l)" -ge 30
done
test "$(find "${THEME_ROOT}" -mindepth 1 -maxdepth 1 -type d | wc -l)" -eq 2
grep -q '^resolution = "1280x720"$' "${CONFIG}"
grep -q '^theme = "Default"$' "${CONFIG}"
grep -q '^background_selection = "ProjectM Fusion"$' "${CONFIG}"
test -z "$(find "${SEED}/firmware" -type f -print -quit)"
test "$(find "${SEED}/internal-games" -mindepth 2 -maxdepth 2 -type d | wc -l)" -eq 2
test "$(head -n 1 "${SNAPSHOT}/build_info")" = "${SNAP_NAME}"
grep -qx 'VERSION=1.0.3' "${SNAPSHOT}/etc/playfusion-release"
"${SCRIPT_DIR}/verify-public-rootfs.sh" "${SNAPSHOT}"
"${SCRIPT_DIR}/verify-public-rootfs.sh" "${SEED}"

# A nodatacow target cannot accept inherited btrfs compression properties.
# Fail the release build before creating an installer if any future staging
# process introduces one outside the cleaned Waydroid state.
for release_tree in "${SNAPSHOT}" "${SEED}"; do
    if getfattr --absolute-names -R -h -m '^btrfs\.compression$' -d \
        "${release_tree}" 2>/dev/null | grep -q '^# file:'; then
        echo "Release data contains incompatible btrfs.compression attributes: ${release_tree}" >&2
        exit 1
    fi
done

echo "[5/6] Creating Btrfs deployment stream..."
btrfs property set -ts "${SNAPSHOT}" ro true
btrfs send -f "${STREAM}" "${SNAPSHOT}"

echo "[6/6] Compressing deployment archive..."
tar -C "${OUTPUT_DIR}" -c -I 'xz -T0 -0' \
    -f "${ARCHIVE}" "$(basename "${STREAM}")"
sha256sum "${ARCHIVE}" > "${OUTPUT_DIR}/sha256sum.txt"
rm -f -- "${STREAM}"

echo "SNAPSHOT_ARCHIVE=${ARCHIVE}"
echo "SEED_DIR=${SEED}"
du -sh "${ARCHIVE}" "${SEED}"
cat "${OUTPUT_DIR}/sha256sum.txt"
