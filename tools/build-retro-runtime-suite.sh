#!/usr/bin/env bash

set -euo pipefail

if [[ $# -lt 3 || $# -gt 4 ]]; then
    echo "Usage: $0 BASE_KZR CORE_ARCHIVE OUTPUT_DIR [WORK_DIR]" >&2
    exit 2
fi

BASE_KZR="$(readlink -f "$1")"
CORE_ARCHIVE="$(readlink -f "$2")"
OUTPUT_DIR="$(mkdir -p "$3" && readlink -f "$3")"
WORK_DIR="${4:-/var/tmp/playfusion-retro-runtime-build}"

for command in fsck.erofs mkfs.erofs bsdtar unzip curl sha256sum; do
    command -v "$command" >/dev/null || {
        echo "Missing required command: $command" >&2
        exit 1
    }
done

[[ -f "$BASE_KZR" ]] || {
    echo "Base runtime not found: $BASE_KZR" >&2
    exit 1
}
[[ -f "$CORE_ARCHIVE" ]] || {
    echo "RetroArch core archive not found: $CORE_ARCHIVE" >&2
    exit 1
}

readonly CORE_PREFIX="RetroArch-Linux-x86_64/RetroArch-Linux-x86_64.AppImage.home/.config/retroarch/cores"
readonly CACHE_DIR="$WORK_DIR/cache"
readonly BASE_DIR="$WORK_DIR/base"
readonly STAGE_DIR="$WORK_DIR/stage"
readonly INFO_ZIP="$CACHE_DIR/info.zip"
readonly PPSSPP_ZIP="$CACHE_DIR/PPSSPP.zip"
readonly SCUMMVM_ZIP="$CACHE_DIR/ScummVM.zip"
readonly AZAHAR_ZIP="$CACHE_DIR/azahar-libretro-linux-x86_64-2125.1.3.zip"

declare -a RUNTIMES=(
    "psp-1.0:ppsspp"
    "gameboy-1.0:mgba"
    "gameboycolor-1.0:mgba"
    "gameboyadvance-1.0:mgba"
    "nintendods-1.0:melondsds"
    "arcade-fbneo-1.0:fbneo"
    "arcade-mame-1.0:mame"
    "mastersystem-1.0:genesis_plus_gx"
    "gamegear-1.0:genesis_plus_gx"
    "sega32x-1.0:picodrive"
    "atari2600-1.0:stella"
    "atari7800-1.0:prosystem"
    "atarilynx-1.0:handy"
    "dosbox-1.0:dosbox_pure"
    "scummvm-1.0:scummvm"
    "amiga-1.0:puae"
    "commodore64-1.0:vice_x64sc"
    "jaguar-1.0:virtualjaguar"
    "nintendo3ds-1.0:azahar"
)

download() {
    local url="$1"
    local destination="$2"
    if [[ ! -s "$destination" ]]; then
        curl -fL --retry 3 --progress-bar -o "$destination" "$url"
    fi
}

extract_core() {
    local core="$1"
    local destination="$2"
    if [[ "$core" == "azahar" ]]; then
        unzip -p "$AZAHAR_ZIP" azahar_libretro.so > "$destination"
        chmod 0644 "$destination"
        return
    fi
    local member="$CORE_PREFIX/${core}_libretro.so"
    bsdtar -xOf "$CORE_ARCHIVE" "$member" > "$destination"
    chmod 0644 "$destination"
}

write_default_config() {
    local destination="$1"
    cat > "$destination" <<'EOF'
audio_driver = "pulse"
input_autodetect_enable = "true"
input_joypad_driver = "sdl2"
input_exit_emulator = "escape"
input_menu_toggle = "f1"
input_save_state = "f2"
input_load_state = "f4"
menu_driver = "ozone"
savefile_directory = "~/.config/retroarch/saves"
savestate_directory = "~/.config/retroarch/states"
system_directory = "~/.config/retroarch/system"
video_driver = "vulkan"
video_fullscreen = "false"
video_threaded = "true"
EOF
}

write_runtime_launcher() {
    local destination="$1"
    local core="$2"
    cat > "$destination" <<EOF
#!/usr/bin/env bash
set -euo pipefail
content="\$(cat "\$1")"

# Make user-supplied firmware available to every RetroArch runtime without
# duplicating copyrighted files inside every cart or runtime image.
central_firmware="/var/kazeta/firmware/retroarch"
runtime_system=".config/retroarch/system"
if [[ -d "\$central_firmware" ]]; then
    while IFS= read -r -d '' source; do
        relative="\${source#"\$central_firmware/"}"
        mkdir -p "\$runtime_system/\$(dirname "\$relative")"
        ln -sfn "\$source" "\$runtime_system/\$relative"
    done < <(find "\$central_firmware" -type f -print0)
fi

exec ./RetroArch.AppImage --libretro ".config/retroarch/cores/${core}_libretro.so" "\$content"
EOF
    chmod 0755 "$destination"
}

write_runtime_metadata() {
    local destination="$1"
    local runtime="$2"
    local core="$3"
    local license=""
    if [[ -f "$destination/.config/retroarch/cores/${core}_libretro.info" ]]; then
        license="$(grep -m1 '^license = ' "$destination/.config/retroarch/cores/${core}_libretro.info" || true)"
    fi
    cat > "$destination/.kazeta/share/runtime-info.txt" <<EOF
PlayFusion runtime: $runtime
Frontend: RetroArch 1.22.2
Core: ${core}_libretro
Core source: https://github.com/libretro
Core binary source: https://buildbot.libretro.com/stable/1.22.2/linux/x86_64/RetroArch_cores.7z
$license
EOF
}

rm -rf "$BASE_DIR" "$STAGE_DIR"
mkdir -p "$CACHE_DIR" "$BASE_DIR" "$STAGE_DIR" "$OUTPUT_DIR"

download "https://buildbot.libretro.com/assets/frontend/info.zip" "$INFO_ZIP"
download "https://buildbot.libretro.com/assets/system/PPSSPP.zip" "$PPSSPP_ZIP"
download "https://buildbot.libretro.com/assets/system/ScummVM.zip" "$SCUMMVM_ZIP"
download \
    "https://github.com/azahar-emu/azahar/releases/download/2125.1.3/azahar-libretro-linux-x86_64-2125.1.3.zip" \
    "$AZAHAR_ZIP"

fsck.erofs --extract="$BASE_DIR" "$BASE_KZR" >/dev/null
find "$BASE_DIR" -exec touch {} +

# Direct-launch runtimes do not need RetroArch's menu artwork, databases,
# cheats, thumbnails, overlays, filters, or shader collections. Keeping these
# out makes each system runtime small while preserving controller autoconfig.
rm -rf \
    "$BASE_DIR/.config/retroarch/assets" \
    "$BASE_DIR/.config/retroarch/cheats" \
    "$BASE_DIR/.config/retroarch/database" \
    "$BASE_DIR/.config/retroarch/downloads" \
    "$BASE_DIR/.config/retroarch/filters" \
    "$BASE_DIR/.config/retroarch/logs" \
    "$BASE_DIR/.config/retroarch/overlays" \
    "$BASE_DIR/.config/retroarch/playlists" \
    "$BASE_DIR/.config/retroarch/records" \
    "$BASE_DIR/.config/retroarch/records_config" \
    "$BASE_DIR/.config/retroarch/screenshots" \
    "$BASE_DIR/.config/retroarch/shaders" \
    "$BASE_DIR/.config/retroarch/thumbnails"

rm -rf "$BASE_DIR/.config/retroarch/cores"
rm -rf "$BASE_DIR/.config/retroarch/config"
mkdir -p \
    "$BASE_DIR/.config/retroarch/cores" \
    "$BASE_DIR/.config/retroarch/config" \
    "$BASE_DIR/.config/retroarch/saves" \
    "$BASE_DIR/.config/retroarch/states" \
    "$BASE_DIR/.config/retroarch/system" \
    "$BASE_DIR/.kazeta/share/licenses"
write_default_config "$BASE_DIR/.config/retroarch/retroarch.cfg"

built_count=0
for specification in "${RUNTIMES[@]}"; do
    runtime="${specification%%:*}"
    core="${specification##*:}"
    if [[ -n "${ONLY_RUNTIME:-}" && "$runtime" != "$ONLY_RUNTIME" ]]; then
        continue
    fi
    stage="$STAGE_DIR/$runtime"
    output="$OUTPUT_DIR/$runtime.kzr"

    echo "Building $runtime with $core..."
    rm -rf "$stage"
    cp -a "$BASE_DIR" "$stage"

    extract_core "$core" "$stage/.config/retroarch/cores/${core}_libretro.so"
    unzip -p "$INFO_ZIP" "${core}_libretro.info" \
        > "$stage/.config/retroarch/cores/${core}_libretro.info" 2>/dev/null || true

    case "$core" in
        ppsspp)
            unzip -q "$PPSSPP_ZIP" -d "$stage/.config/retroarch/system"
            ;;
        scummvm)
            unzip -q "$SCUMMVM_ZIP" -d "$stage/.config/retroarch/system"
            ;;
    esac

    write_runtime_launcher "$stage/.kazeta/share/run" "$core"
    write_runtime_metadata "$stage" "$runtime" "$core"

    rm -f "$output"
    mkfs.erofs -zlzma "$output" "$stage" >/dev/null
    fsck.erofs "$output" >/dev/null
    file "$output"
    sha256sum "$output"
    built_count=$((built_count + 1))
done

echo "Built $built_count separate PlayFusion runtimes in $OUTPUT_DIR"
