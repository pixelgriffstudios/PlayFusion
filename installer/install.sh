#!/usr/bin/env bash
set -euo pipefail

OS_NAME="PlayFusion 1.0.2"
MIN_DISK_SIZE=28
MOUNT_PATH=/tmp/frzr_root
SEED_ARCHIVE=/root/playfusion-seed.tar

enable_all_gamepads() {
    busctl set-property org.shadowblip.InputPlumber \
        /org/shadowblip/InputPlumber/Manager \
        org.shadowblip.InputManager ManageAllDevices b 1 >/dev/null 2>&1
}

load_gamepad_profile() {
    busctl call org.shadowblip.InputPlumber \
        /org/shadowblip/InputPlumber/CompositeDevice0 \
        org.shadowblip.Input.CompositeDevice \
        LoadProfilePath s /root/gamepad_profile.yaml >/dev/null 2>&1
}

poll_gamepad() {
    modprobe xpad >/dev/null 2>&1 || true
    systemctl start inputplumber >/dev/null 2>&1 || true
    while true; do
        sleep 1
        enable_all_gamepads || true
        if load_gamepad_profile; then break; fi
    done
}

get_boot_disk() {
    local source parent current_boot_id boot_disk_info part_uuid part part_path

    source=$(findmnt -n -o SOURCE /run/archiso/bootmnt 2>/dev/null || true)
    if test -n "${source}"; then
        parent=$(lsblk -ndo PKNAME "${source}" 2>/dev/null || true)
        if test -n "${parent}"; then
            echo "${parent}"
            return
        fi
    fi

    current_boot_id=$(efibootmgr 2>/dev/null | awk -F: '/BootCurrent/{gsub(/ /,"",$2); print $2; exit}')
    boot_disk_info=$(efibootmgr 2>/dev/null | grep "Boot${current_boot_id}" | head -1 || true)
    part_uuid=$(echo "${boot_disk_info}" | tr / '\n' | grep 'HD(' | cut -d, -f3 | head -1 | sed 's/^0x//' || true)
    test -n "${part_uuid}" || return 0
    part=$(blkid | grep -i "${part_uuid}" | cut -d: -f1 | head -1 | sed 's,/dev/,,' || true)
    test -n "${part}" || return 0
    part_path=$(readlink "/sys/class/block/${part}")
    basename "$(dirname "${part_path}")"
}

is_disk_external() {
    local disk=$1
    test "$(lsblk -dn -o HOTPLUG "/dev/${disk}" | xargs)" = 1
}

disk_bytes() {
    lsblk -bdn -o SIZE "/dev/$1" | xargs
}

disk_description() {
    local disk=$1 size model vendor transport
    size=$(lsblk -dn -o SIZE "/dev/${disk}" | xargs)
    model=$(lsblk -dn -o MODEL "/dev/${disk}" | xargs)
    vendor=$(lsblk -dn -o VENDOR "/dev/${disk}" | xargs)
    transport=$(lsblk -dn -o TRAN "/dev/${disk}" | xargs | sed \
        -e 's/^usb$/USB/' -e 's/^nvme$/Internal/' \
        -e 's/^sata$/Internal/' -e 's/^ata$/Internal/' \
        -e 's/^mmc$/SD card/')
    printf '[%s] %s %s (%s)\n' "${transport:-Unknown}" "${vendor}" \
        "${model:-Unknown model}" "${size}"
}

cancel_install() {
    if whiptail --yesno --yes-button "Power off" --no-button "Open command prompt" \
        "Installation was cancelled. What would you like to do?" 10 70; then
        poweroff
    fi
    exec /usr/bin/zsh -f
}

fail_install() {
    whiptail --msgbox \
        "Installation could not be completed.\n\n$1\n\nA command prompt will now open." 15 76
    exec /usr/bin/zsh -f
}

prepare_persistent_user_data() {
    local persistent_dir="${MOUNT_PATH}/var/kazeta/user-data/kazeta-plus"
    local user_share="${DEPLOYMENT}/home/gamer/.local/share"
    local user_data="${user_share}/kazeta-plus"
    local immutable_defaults="${user_share}/kazeta-plus.immutable-defaults"

    mkdir -p "${persistent_dir}" "${user_share}"

    if [[ -L "${user_data}" ]]; then
        [[ $(readlink "${user_data}") == /var/kazeta/user-data/kazeta-plus ]] || return 1
        if [[ -d "${immutable_defaults}" ]] &&
            [[ -z $(find "${persistent_dir}" -mindepth 1 -print -quit) ]]; then
            cp -a "${immutable_defaults}/." "${persistent_dir}/"
        fi
    elif [[ -d "${user_data}" ]]; then
        cp -a "${user_data}/." "${persistent_dir}/"
        [[ ! -e "${immutable_defaults}" ]] || return 1
        mv "${user_data}" "${immutable_defaults}"
        ln -s /var/kazeta/user-data/kazeta-plus "${user_data}"
    elif [[ -e "${user_data}" ]]; then
        return 1
    else
        ln -s /var/kazeta/user-data/kazeta-plus "${user_data}"
    fi

    chown -R 1000:1000 "${persistent_dir}"
    chown 1000:1000 \
        "${DEPLOYMENT}/home/gamer" \
        "${DEPLOYMENT}/home/gamer/.local" \
        "${user_share}"
    chown -h 1000:1000 "${user_data}"
    find "${persistent_dir}" -type d -exec chmod u+rwx,go+rx {} \;
    find "${persistent_dir}" -type f -exec chmod u+rw,go+r {} \;
}

select_disk() {
    local boot_disk line name description
    while true; do
        local choices=()
        boot_disk=$(get_boot_disk)
        while read -r line; do
            name=$(awk '{print $1}' <<<"${line}")
            test -n "${name}" || continue
            test "${name}" != "${boot_disk}" || continue
            description=$(disk_description "${name}")
            choices+=("${name}" "${description}")
        done < <(lsblk -dn -o NAME,TYPE | awk '$2=="disk" && $1!~/^zram/{print}')

        if test "${#choices[@]}" -gt 2; then
            DISK=$(whiptail --nocancel --menu \
                "Choose an internal disk to install ${OS_NAME} on:" \
                20 78 7 "${choices[@]}" 3>&1 1>&2 2>&3)
        elif test "${#choices[@]}" -eq 2; then
            DISK=${choices[0]}
        else
            whiptail --msgbox \
                "No install target was found.\n\nConnect a 32 GB or larger disk and restart the installer." 12 72
            cancel_install
        fi

        DISK_DESC=$(disk_description "${DISK}")
        if test "$(disk_bytes "${DISK}")" -lt $((MIN_DISK_SIZE * 1000 * 1000 * 1000)); then
            whiptail --msgbox \
                "${DISK} is too small. PlayFusion requires a 32 GB or larger target." 11 72
            continue
        fi

        if is_disk_external "${DISK}"; then
            if ! whiptail --yesno --defaultno --yes-button "Install anyway" \
                --no-button "Choose another disk" \
                "${DISK} appears to be removable storage. Install anyway?" 12 72; then
                continue
            fi
        fi
        break
    done
}

test "$(id -u)" -eq 0
test -f "${SEED_ARCHIVE}" || fail_install "Factory data is missing from the installer."

dmesg --console-level 1
poll_gamepad &
select_disk

if ! whiptail --yesno --defaultno --yes-button "Erase disk and install" \
    --no-button "Cancel" \
    "WARNING: All data on this disk will be erased:\n\n${DISK} - ${DISK_DESC}\n\nPlayFusion will use the full disk after installation. Continue?" \
    17 78; then
    cancel_install
fi

frzr-bootstrap gamer "/dev/${DISK}" || fail_install "Disk preparation failed."

BASE_IMAGE=$(find /root -maxdepth 1 -type f -name '*.img.tar.xz' -print -quit)
test -n "${BASE_IMAGE}" || fail_install "The PlayFusion system snapshot is missing."

export SHOW_UI=1
frzr-deploy "${BASE_IMAGE}" || fail_install "The PlayFusion deployment failed."

DEPLOYMENT_NAME=$(basename "${BASE_IMAGE}" .img.tar.xz)
DEPLOYMENT="${MOUNT_PATH}/deployments/${DEPLOYMENT_NAME}"
test -d "${DEPLOYMENT}" || fail_install "The installed deployment could not be found."

# Received deployments are read-only snapshots.  Generate unique SSH host
# keys for this installation before sealing the deployment again.  Shared
# public host keys would be insecure, while omitting them prevents SSH/SFTP.
btrfs property set -ts "${DEPLOYMENT}" ro false
# The small installer environment intentionally does not carry OpenSSH.  Use
# the deployed system's ssh-keygen instead.  It needs the live /dev mounted in
# the chroot because OpenSSH opens /dev/null while generating the keys.
mount --bind /dev "${DEPLOYMENT}/dev" || fail_install "Could not prepare SSH host-key generation."
if ! chroot "${DEPLOYMENT}" /usr/bin/ssh-keygen -q -A; then
    umount "${DEPLOYMENT}/dev" || true
    fail_install "SSH host-key generation failed."
fi
umount "${DEPLOYMENT}/dev" || fail_install "Could not finish SSH host-key generation."
test -s "${DEPLOYMENT}/etc/ssh/ssh_host_ed25519_key" || \
    fail_install "The SSH host keys were not created."
btrfs property set -ts "${DEPLOYMENT}" ro true

echo "Installing clean factory data..."
mkdir -p "${MOUNT_PATH}/var/kazeta"
find "${MOUNT_PATH}/var/kazeta" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +
tar --numeric-owner --acls --xattrs -xpf "${SEED_ARCHIVE}" \
    -C "${MOUNT_PATH}/var/kazeta"
# The top directory already existed as a mount path, so set its known-good
# owner explicitly even though all child metadata came from the archive.
chown 1000:1000 "${MOUNT_PATH}/var/kazeta"
chmod 0755 "${MOUNT_PATH}/var/kazeta"

# The release seed intentionally contains empty save/profile directories. Tar
# preserves their staging ownership, which can be root even though the runtime
# creates each game's writable overlay as the gamer account. Normalize the
# writable state roots here so the very first game launch can create its save
# directory on a freshly installed system.
for writable_dir in \
    saves saves/default saves/profiles cache controller-profiles profiles state; do
    if [[ -e "${MOUNT_PATH}/var/kazeta/${writable_dir}" ]]; then
        chown -R 1000:1000 "${MOUNT_PATH}/var/kazeta/${writable_dir}"
    fi
done

# Category folders must remain writable by gamer and the FTP/SFTP group so new
# internal games can be installed after first boot. Resolve the group from the
# installed deployment instead of assuming a fixed numeric GID.
KAZETAFTP_GID="$(awk -F: '$1 == "kazetaftp" { print $3; exit }' "${DEPLOYMENT}/etc/group")"
[[ "${KAZETAFTP_GID}" =~ ^[0-9]+$ ]] || \
    fail_install "Could not resolve the installed kazetaftp group."
while IFS= read -r -d '' category_dir; do
    chown 1000:"${KAZETAFTP_GID}" "${category_dir}"
    chmod 2775 "${category_dir}"
done < <(find "${MOUNT_PATH}/var/kazeta/internal-games" \
    -mindepth 1 -maxdepth 1 -type d -print0)

# PlayFusion's menu writes settings and user-installed themes below
# ~/.local/share/kazeta-plus.  The deployed system is deliberately sealed
# read-only, so migrate that directory into the persistent /var subvolume and
# retain the path expected by the application through a directory symlink.
btrfs property set -ts "${DEPLOYMENT}" ro false || \
    fail_install "Could not prepare persistent PlayFusion settings."
if ! prepare_persistent_user_data; then
    btrfs property set -ts "${DEPLOYMENT}" ro true || true
    fail_install "Persistent PlayFusion settings could not be initialized."
fi
btrfs property set -ts "${DEPLOYMENT}" ro true || \
    fail_install "Could not reseal the PlayFusion deployment."

test "$(find "${MOUNT_PATH}/var/kazeta/internal-games" -mindepth 2 -maxdepth 2 -type d | wc -l)" -eq 2 || \
    fail_install "The public game library failed validation."
test "$(stat -c '%u:%g' "${MOUNT_PATH}/var/kazeta")" = 1000:1000 || \
    fail_install "Factory data ownership validation failed."
test "$(stat -c '%u:%g' "${MOUNT_PATH}/var/kazeta/saves/default")" = 1000:1000 || \
    fail_install "Factory save-directory ownership validation failed."
test -L "${DEPLOYMENT}/home/gamer/.local/share/kazeta-plus" || \
    fail_install "Persistent settings link validation failed."
test "$(readlink "${DEPLOYMENT}/home/gamer/.local/share/kazeta-plus")" = \
    /var/kazeta/user-data/kazeta-plus || \
    fail_install "Persistent settings target validation failed."
test "$(stat -c '%u:%g' "${MOUNT_PATH}/var/kazeta/user-data/kazeta-plus")" = 1000:1000 || \
    fail_install "Persistent settings ownership validation failed."

printf '%s\n' 'playfusion/local' > "${MOUNT_PATH}/source"
sync

MESSAGE="PlayFusion 1.0.2 installed successfully.

Included:
  - All 38 system runtimes
  - Hell on Rails
  - PlayFusion Arcade
  - 30 years music album
  - One clean Default profile

The installed system has no personal saves or user profiles."

if whiptail --yesno --yes-button "Reboot" --no-button "Open command prompt" \
    "${MESSAGE}" 22 76; then
    reboot
fi
exec /usr/bin/zsh -f
