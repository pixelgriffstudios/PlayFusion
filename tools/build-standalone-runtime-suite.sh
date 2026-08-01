#!/usr/bin/env bash

set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
    echo "Usage: $0 OUTPUT_DIR [WORK_DIR]" >&2
    exit 2
fi

OUTPUT_DIR="$(mkdir -p "$1" && readlink -f "$1")"
WORK_DIR="${2:-/var/tmp/playfusion-standalone-runtime-build}"
CACHE_DIR="$WORK_DIR/cache"
STAGE_DIR="$WORK_DIR/stage"

for command in curl fsck.erofs mkfs.erofs sha256sum; do
    command -v "$command" >/dev/null || {
        echo "Missing required command: $command" >&2
        exit 1
    }
done

mkdir -p "$CACHE_DIR" "$STAGE_DIR" "$OUTPUT_DIR"

download() {
    local url="$1"
    local destination="$2"
    if [[ ! -s "$destination" ]]; then
        curl -fL --retry 3 --progress-bar -o "$destination" "$url"
    fi
}

package_runtime() {
    local name="$1"
    local stage="$2"
    local output="$OUTPUT_DIR/$name.kzr"
    rm -f "$output"
    find "$stage" -exec touch {} +
    mkfs.erofs -zlzma "$output" "$stage" >/dev/null
    fsck.erofs "$output" >/dev/null
    file "$output"
    sha256sum "$output"
}

VITA_APPIMAGE="$CACHE_DIR/Vita3K-x86_64.AppImage"
XEMU_APPIMAGE="$CACHE_DIR/xemu-0.8.136-x86_64.AppImage"

download \
    "https://github.com/Vita3K/Vita3K/releases/download/continuous/Vita3K-x86_64.AppImage" \
    "$VITA_APPIMAGE"
download \
    "https://github.com/xemu-project/xemu/releases/download/v0.8.136/xemu-0.8.136-x86_64.AppImage" \
    "$XEMU_APPIMAGE"
chmod 0755 "$VITA_APPIMAGE" "$XEMU_APPIMAGE"

vita_stage="$STAGE_DIR/playstationvita-1.0"
rm -rf "$vita_stage"
mkdir -p "$vita_stage/.kazeta/share/licenses"
cp "$VITA_APPIMAGE" "$vita_stage/Vita3K.AppImage"
cat > "$vita_stage/.kazeta/share/run" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

content="$(cat "$1")"
if [[ ! -e "$content" ]]; then
    zenity --error --title="PlayFusion - Vita3K" \
        --text="Vita game content was not found:\n$content" 2>/dev/null || true
    exit 2
fi

# Vita3K accepts a decrypted app directory or supported installable
# .vpk/.zip content as a positional path. Its writable firmware, app install,
# shader cache, and save data remain in this game's Kazeta overlay through HOME.
exec ./Vita3K.AppImage \
    --console \
    --fullscreen \
    --backend-renderer Vulkan \
    -- "$content"
EOF
chmod 0755 "$vita_stage/.kazeta/share/run"
cat > "$vita_stage/.kazeta/share/runtime-info.txt" <<'EOF'
PlayFusion runtime: playstationvita-1.0
Emulator: Vita3K continuous release
Binary source: https://github.com/Vita3K/Vita3K/releases/tag/continuous
License/source: https://github.com/Vita3K/Vita3K
Firmware is not bundled. Install Sony's official PS Vita firmware and font
package into the per-game Vita3K data directory before launching commercial
software.
EOF
package_runtime "playstationvita-1.0" "$vita_stage"

xemu_stage="$STAGE_DIR/xbox-1.0"
rm -rf "$xemu_stage"
mkdir -p "$xemu_stage/.kazeta/share/licenses"
cp "$XEMU_APPIMAGE" "$xemu_stage/xemu.AppImage"
cat > "$xemu_stage/.kazeta/share/run" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

content="$(cat "$1")"
system_dir="${PLAYFUSION_FIRMWARE_DIR:-/var/kazeta/firmware}/xbox"
bootrom="$system_dir/mcpx_1.0.bin"
flashrom="$system_dir/Complex_4627.bin"
hdd="$system_dir/xbox_hdd.qcow2"
data_dir="$HOME/.local/share/xemu/xemu"
eeprom="$data_dir/eeprom.bin"
config="$data_dir/xemu.toml"

missing=()
[[ -f "$content" ]] || missing+=("Xbox XISO: $content")
[[ -f "$bootrom" ]] || missing+=("MCPX: $bootrom")
[[ -f "$flashrom" ]] || missing+=("BIOS: $flashrom")
[[ -f "$hdd" ]] || missing+=("HDD image: $hdd")
if ((${#missing[@]})); then
    message="xemu cannot start because these files are missing:"
    for item in "${missing[@]}"; do message+=$'\n'"  • $item"; done
    zenity --error --title="PlayFusion - Original Xbox" \
        --width=680 --text="$message" 2>/dev/null || true
    printf '%s\n' "$message" >&2
    exit 2
fi

mkdir -p "$data_dir"
cat > "$config" <<CFG
[general]
show_welcome = false
skip_boot_anim = true

[general.updates]
check = false

[input]
auto_bind = true
background_input_capture = false

[display]
renderer = "OPENGL"

[display.quality]
surface_scale = 1

[display.window]
fullscreen_on_startup = true
fullscreen_exclusive = false
startup_size = "1920x1080"
vsync = true

[display.ui]
show_menubar = false
show_notifications = true
hide_cursor = true
fit = "scale"
aspect_ratio = "auto"

[sys]
mem_limit = "64"
avpack = "hdtv"

[sys.files]
bootrom_path = "$bootrom"
flashrom_path = "$flashrom"
eeprom_path = "$eeprom"
hdd_path = "$hdd"
dvd_path = ""

[perf]
hard_fpu = true
cache_shaders = true
CFG

exec ./xemu.AppImage \
    -full-screen \
    -config_path "$config" \
    -dvd_path "$content"
EOF
chmod 0755 "$xemu_stage/.kazeta/share/run"
cat > "$xemu_stage/.kazeta/share/runtime-info.txt" <<'EOF'
PlayFusion runtime: xbox-1.0
Emulator: xemu 0.8.136
Binary source: https://github.com/xemu-project/xemu/releases/tag/v0.8.136
License/source: https://github.com/xemu-project/xemu
Required user-supplied files under /var/kazeta/firmware/xbox:
  mcpx_1.0.bin
  Complex_4627.bin
  xbox_hdd.qcow2
Only Xbox-compatible XISO game images are supported.
EOF
package_runtime "xbox-1.0" "$xemu_stage"

echo "Built standalone PlayFusion runtimes in $OUTPUT_DIR"
