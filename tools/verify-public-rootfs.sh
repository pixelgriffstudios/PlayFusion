#!/usr/bin/env bash
set -euo pipefail

# Abort a public PlayFusion build if a staged root filesystem or factory-data
# seed contains user-supplied console firmware, decryption keys, or save data.
root=${1:?Usage: verify-public-rootfs.sh ROOTFS}
root=$(realpath "$root")
[[ -d "$root" && "$root" != / ]] || {
    echo "Refusing unsafe or missing rootfs path: $root" >&2
    exit 2
}

failures=()
while IFS= read -r -d '' file; do
    relative=${file#"$root"/}
    lower=${relative,,}
    base=${lower##*/}
    case "$base" in
        scph*.bin|bios_cd_u.bin|bios_cd_e.bin|bios_cd_j.bin|sega_101.bin|mpr-17933.bin|syscard3.pce|lynxboot.img|'7800 bios (u).rom'|bios7.bin|bios9.bin|firmware.bin|dsi_bios7.bin|dsi_bios9.bin|dsi_firmware.bin|dsi_nand.bin|gb_bios.bin|gbc_bios.bin|gba_bios.bin|sgb_bios.bin|kick*.rom|kick*.bin|kick*.a500|aes_keys.txt|mcpx_1.0.bin|complex_4627.bin|complex_4627v1.03.bin|xbox_hdd.qcow2|dc_boot.bin|dc_flash.bin|psvupdat.pup|psp2updat.pup|otp.bin|seeprom.bin)
            failures+=("$relative")
            ;;
        keys.txt)
            [[ "$lower" == *'/firmware/wiiu/'* || "$lower" == *'/cemu/'* ]] && failures+=("$relative")
            ;;
    esac
done < <(find "$root" -type f -print0)

# Theme/update archives are public payloads too. Inspect their member names so
# a private key or BIOS cannot bypass the filename scan merely by being zipped.
if command -v bsdtar >/dev/null 2>&1; then
    while IFS= read -r -d '' archive; do
        while IFS= read -r member; do
            lower=${member,,}
            base=${lower##*/}
            case "$base" in
                scph*.bin|bios_cd_u.bin|bios_cd_e.bin|bios_cd_j.bin|sega_101.bin|mpr-17933.bin|syscard3.pce|aes_keys.txt|mcpx_1.0.bin|complex_4627*.bin|dc_boot.bin|dc_flash.bin|otp.bin|seeprom.bin)
                    failures+=("${archive#"$root"/}:$member")
                    ;;
                keys.txt)
                    [[ "$lower" == *'/firmware/wiiu/'* || "$lower" == *'/cemu/'* ]] && \
                        failures+=("${archive#"$root"/}:$member")
                    ;;
            esac
        done < <(bsdtar -tf "$archive" 2>/dev/null || true)
    done < <(find "$root" -type f -iname '*.zip' -print0)
fi

# A cloned developer system must not publish personal state.
for personal in \
    var/kazeta/saves \
    var/kazeta/profiles \
    var/kazeta/firmware; do
    if [[ -d "$root/$personal" ]] && find "$root/$personal" -type f -print -quit | grep -q .; then
        failures+=("$personal contains personal files")
    fi
done

# The verifier is also called directly on the factory seed, where these roots
# are not prefixed with var/kazeta.  An empty firmware hierarchy and the single
# generated default profile are allowed; saves must contain no files.
if [[ -d "$root/firmware" ]] && find "$root/firmware" -type f -print -quit | grep -q .; then
    failures+=("firmware contains private files")
fi
if [[ -d "$root/saves" ]] && find "$root/saves" -type f -print -quit | grep -q .; then
    failures+=("saves contains personal files")
fi
if [[ -d "$root/profiles" ]]; then
    while IFS= read -r profile; do
        [[ "${profile##*/}" == default.toml ]] || failures+=("${profile#"$root"/}")
    done < <(find "$root/profiles" -maxdepth 1 -type f -print)
fi

if ((${#failures[@]})); then
    echo "PUBLIC RELEASE BLOCKED: private firmware, keys or user state found:" >&2
    printf '  - %s\n' "${failures[@]}" >&2
    exit 1
fi

echo "Public rootfs guard passed: no private firmware, keys, saves or profiles found."
