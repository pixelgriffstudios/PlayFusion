# PlayFusion themes

PlayFusion 1.0.2 accepts native PlayFusion themes and remains backward
compatible with Kazeta+ themes. A theme is a folder containing `theme.toml`.
Missing PlayFusion-only fields use safe defaults, so an older theme does not
need to be rewritten before it can be imported.

## Install from USB or SD

Either place an unpacked theme folder anywhere within five folders of the
removable drive's root, or place one or more theme ZIP files on the drive.
For a tidy layout, this structure is recommended:

```text
themes/
  MyTheme/
    theme.toml
    background.png
    logo.png
    menu.ttf
    music.ogg
    boot_animation.mp4       # optional
    system-folders/          # optional
      nes.png
      snes.png
      playstation.png
```

Then open **Extras > Theme Management > Import Theme from USB / SD**. Apply an
installed theme from the same screen. **Reset to PlayFusion Default Theme**
restores the factory appearance without changing games, saves, profiles,
networking, resolution, audio, or storage.

The importer refuses archive traversal, symbolic links, more than 2,000 files,
and archives whose expanded content exceeds 256 MiB. Existing themes with the
same folder name are replaced only after the new copy has been safely staged.

## `theme.toml`

This complete example includes the optional PlayFusion 1.0.2 fields:

```toml
author = "Your name"
description = "Short description"
menu_position = "TopRight"
profile_badge_position = "LEFT"
font_color = "WHITE"
cursor_color = "YELLOW"
cursor_style = "TEXT"
cursor_blink_speed = "OFF"
cursor_transition_speed = "OFF"
background_scroll_speed = "OFF"
color_shift_speed = "OFF"
bgm_track = "music.ogg"
logo_selection = "logo.png"
background_selection = "background.png"
font_selection = "menu.ttf"
sfx_pack = "sounds"
boot_animation = "boot_animation.mp4"
```

`menu_position` accepts `Center`, `TopLeft`, `TopRight`, `BottomLeft`, or
`BottomRight`. `profile_badge_position` accepts `LEFT` or `RIGHT`; it can also
be changed globally in GUI Settings. Text and the active-profile badge border
use the theme's configured font and highlight colors.

Backgrounds may be static images, compatible MP4 video, or a built-in native
background name such as `Retro Laser Grid` or `Xbox 2.0`. Native backgrounds
render at the current output resolution and avoid video decoding.

## Optional boot animation

Boot animation support is user-space theming; it never changes firmware, the
bootloader, or the installer. PlayFusion accepts only a local basename from the
theme folder and validates it before playback:

- H.264 video
- no larger than 1920x1080
- no longer than 15 seconds
- no larger than 64 MiB
- AAC audio is recommended

If the file is missing, invalid, unsupported, or fails to play, PlayFusion
automatically displays its built-in native splash. The themed animation plays
only once per boot and is not repeated after quitting a game.

## Optional system-folder artwork

Place portrait PNG images in `system-folders/`. Missing images automatically
fall back to PlayFusion's built-in folders. Supported filenames include:

```text
all-games, favorites, recently-played, other, pc-games, android, arcade,
nes, snes, game-boy, game-boy-advance, nintendo-64, nintendo-ds,
nintendo-3ds, gamecube, wii, wii-u, playstation, playstation-2, psp,
playstation-vita, original-xbox, dreamcast, sega-genesis, sega-cd,
sega-32x, sega-saturn, game-gear, atari-2600, atari-7800, atari-lynx,
dos, amiga, commodore-64
```

For compatibility, PlayFusion also checks legacy `system-covers/` and
`folders/` directories.

## Packaging

ZIP the theme's top-level folder, not only its individual contents:

```text
MyTheme.zip
  MyTheme/
    theme.toml
    ...
```

Do not include games, BIOS dumps, encryption keys, commercial media, personal
saves, absolute links, or private signing keys in a public theme.
