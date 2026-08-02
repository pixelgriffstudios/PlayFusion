#!/usr/bin/env bash
set -euo pipefail

THEME_DIR=${1:?Usage: build-xbox-theme-assets.sh THEME_DIR BOOT_CLIP [OUTPUT_ZIP]}
BOOT_CLIP=${2:?Usage: build-xbox-theme-assets.sh THEME_DIR BOOT_CLIP [OUTPUT_ZIP]}
OUTPUT_ZIP=${3:-/tmp/xbox_original-PlayFusion-optimized.zip}
SYSTEM_COVERS=/usr/share/playfusion/system-covers

test -d "$THEME_DIR"
test -d "$SYSTEM_COVERS"
test -f "$BOOT_CLIP"
command -v ffmpeg >/dev/null
command -v ffprobe >/dev/null
command -v bsdtar >/dev/null

video_codec=$(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name \
    -of default=nw=1:nk=1 "$BOOT_CLIP")
video_width=$(ffprobe -v error -select_streams v:0 -show_entries stream=width \
    -of default=nw=1:nk=1 "$BOOT_CLIP")
video_height=$(ffprobe -v error -select_streams v:0 -show_entries stream=height \
    -of default=nw=1:nk=1 "$BOOT_CLIP")
duration=$(ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 "$BOOT_CLIP")
audio_codec=$(ffprobe -v error -select_streams a:0 -show_entries stream=codec_name \
    -of default=nw=1:nk=1 "$BOOT_CLIP")

test "$video_codec" = h264
test "$audio_codec" = aac
test "$video_width" -le 1920
test "$video_height" -le 1080
awk -v duration="$duration" 'BEGIN { exit !(duration > 0 && duration <= 15.0) }'

install -m 0644 "$BOOT_CLIP" "$THEME_DIR/boot_animation.mp4"
mkdir -p "$THEME_DIR/system-folders"

for source in "$SYSTEM_COVERS"/*.png; do
    name=$(basename "$source")
    temporary="$THEME_DIR/system-folders/.${name}.tmp.png"
    destination="$THEME_DIR/system-folders/$name"
    ffmpeg -y -hide_banner -loglevel error -i "$source" \
        -vf "scale=384:512:flags=lanczos,eq=saturation=0.38:contrast=1.06:brightness=-0.055,colorbalance=gs=0.30:gm=0.18:gh=0.12,drawbox=x=5:y=5:w=iw-10:h=ih-10:color=0x52ff68@0.95:t=5,drawbox=x=13:y=13:w=iw-26:h=ih-26:color=0x0b4819@0.75:t=2" \
        -frames:v 1 -compression_level 9 "$temporary"
    mv "$temporary" "$destination"
done

if ! grep -q '^profile_badge_position[[:space:]]*=' "$THEME_DIR/theme.toml"; then
    printf '\nprofile_badge_position = "LEFT"\n' >> "$THEME_DIR/theme.toml"
else
    sed -i 's/^profile_badge_position[[:space:]]*=.*/profile_badge_position = "LEFT"/' \
        "$THEME_DIR/theme.toml"
fi
if ! grep -q '^boot_animation[[:space:]]*=' "$THEME_DIR/theme.toml"; then
    printf 'boot_animation = "boot_animation.mp4"\n' >> "$THEME_DIR/theme.toml"
else
    sed -i 's/^boot_animation[[:space:]]*=.*/boot_animation = "boot_animation.mp4"/' \
        "$THEME_DIR/theme.toml"
fi

# Validate every generated cover and every required theme asset before packing.
test "$(find "$THEME_DIR/system-folders" -maxdepth 1 -type f -name '*.png' | wc -l)" = \
    "$(find "$SYSTEM_COVERS" -maxdepth 1 -type f -name '*.png' | wc -l)"
for required in theme.toml xbox_background.mp4 xbox_logo.png xbox_menu.ttf xbox_bgm.ogg \
    xbox_original_sfx/back.wav xbox_original_sfx/move.wav \
    xbox_original_sfx/reject.wav xbox_original_sfx/select.wav boot_animation.mp4; do
    test -s "$THEME_DIR/$required"
done

parent=$(dirname "$THEME_DIR")
folder=$(basename "$THEME_DIR")
rm -f "$OUTPUT_ZIP"
(cd "$parent" && bsdtar -a -cf "$OUTPUT_ZIP" "$folder")
sha256sum "$OUTPUT_ZIP"
du -h "$OUTPUT_ZIP"
