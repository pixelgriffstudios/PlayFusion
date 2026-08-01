# Internal Games

The Internal Games screen adds a controller-friendly gallery for games stored
on the Kazeta+ system drive. It launches games through the same KZI/runtime
pipeline used by removable Kazeta media.

## Library layout

Store one game per directory under `/var/kazeta/internal-games`:

```text
/var/kazeta/internal-games/
  example-game/
    cart.kzi
    icon.png
    cover.jpg
    game/
      example-game.ext
```

The KZI format supports optional `Cover` and `WiiGamepad` fields:

```ini
Name=Example Game
Id=example-game
Exec=game/example-game.ext
Icon=icon.png
Cover=cover.jpg
Runtime=example-1.0
WiiGamepad=true
```

Keyboard-only Windows or Linux games can opt into PlayFusion's AntiMicroX
controller layer in either of two ways:

```ini
KeyboardMapping=true
```

This uses the enabled per-game profile saved from **Extras → PC Controller
Profiles**, or:

```ini
KeyboardProfile=controller.gamecontroller.amgp
```

This loads a profile shipped beside the KZI. Both fields are ignored for
emulator runtimes so native emulator controller support is never replaced.

`Exec`, `Icon`, and `Cover` must be relative paths that stay inside the game's
directory. If `Cover` is absent or invalid, the gallery uses `Icon`.

For the Dolphin runtime, WBFS/WAD files and Wii ISO headers automatically
receive Vulkan as their renderer and the default Xbox Wii gamepad profile. Set
`WiiGamepad=true` to identify another Wii image format and enable the profile,
or `WiiGamepad=false` when a Wii game should retain a real/emulated Wii Remote
configuration. `WiiGamepad=false` does not disable Vulkan. These defaults are
applied to the active saved overlay at every launch, so an older saved
RetroArch configuration cannot restore OpenGL or the previous rotated
controls.

The Wii launch profile also selects JIT64, dual-core mode, fastmem, asynchronous
shader compilation, and shader-cache preloading. This prevents synchronous
shader compilation from pausing gameplay the first time a new effect appears.

## Controls

- A: launch or confirm
- B: go back
- X: refresh the library
- RB: install a Kazeta game from mounted removable/optical media
- LB: delete the selected internal game after confirmation

## Internal storage expansion

PlayFusion can use additional internal HDDs or SSDs as game-library storage.
External USB drives, removable media, and the disk containing the running
PlayFusion system are deliberately excluded.

Install a SATA drive only while the computer is completely powered off. If the
drive previously contained a bootable operating system, keep the PlayFusion
system disk first in the firmware boot order.

Open `Extras` and select `Storage Expansion`. The screen lists only eligible
secondary internal drives and shows their model, device path, capacity, status,
and available space. Preparing a new drive is destructive:

1. Select the drive and press A.
2. Verify the model, device path, and capacity in the warning.
3. Press Y to erase and prepare it, or B to cancel.

The preparation step creates a GPT partition table and one ext4 filesystem
labeled `PLAYFUSION_GAMES`. Prepared filesystems are mounted by UUID below
`/var/kazeta/storage/`; each contains a `games` directory and a marker used to
validate it as PlayFusion storage.

The Internal Games gallery merges games from the system library and every
mounted expansion drive. When installing a game from a cartridge or optical
disc, PlayFusion asks which library should receive it. Duplicate game IDs are
not loaded twice. Saved games remain on the PlayFusion system disk, so removing
an expansion drive does not remove saves.

## FTP

The main menu displays the current IPv4 address and FTP port. The default
endpoint uses port `2121`:

```text
Host: <address shown in the Kazeta+ menu>
Port: 2121
User: kazetaftp
Password: kazeta
```

The FTP root exposes:

- `internal-games/` for game folders
- `runtimes/` as an upload inbox for `.kzr` files

Uploaded runtime files are accepted only after the transfer is closed and the
file is identified as an EROFS image. Accepted files are installed in
`/usr/share/kazeta/runtimes`.

Change the default FTP password after first boot on any console connected to an
untrusted network:

```bash
sudo passwd kazetaftp
```

## Cover handling

The gallery decodes common image formats through the Rust `image` crate.
Additionally, a background normalizer converts non-PNG cover images to a safe
PNG cache. An unreadable cover is removed from the KZI metadata so the icon is
used instead of terminating the BIOS session.

## Console optical discs

The optical-disc helper distinguishes normal Kazeta data discs from supported
original or properly burned console discs. It derives a stable disc ID and
prepares a temporary KZI-compatible cart under
`/run/media/kazeta-optical`. The same gallery action can install a recognized
disc to the main library, an expansion drive, or a removable ext4 cart.

PS1 Mode-2 tracks are exposed through `kazeta-cdstream`, a read-only FUSE
bridge backed by Linux `CDROMREADRAW`. The bridge presents the physical disc
as a seekable 2,352-byte-sector BIN plus a track-aware CUE. Data and audio
tracks retain their TOC positions, so mixed-mode console discs do not need to
be copied internally before play. If streaming cannot start, the previous
`cdrdao` raw BIN/TOC cache remains available as a fallback.
The helper uses the installed `playstation-1.01.kzr` runtime and a user-supplied
`scph5501.bin` BIOS. It never downloads or distributes console firmware.

The main-menu `PLAY` option stays disabled and gray while the disc is being
read. Only after the raw image, TOC, runtime, BIOS, icon, and KZI metadata are
ready does the helper expose the virtual cart; the existing media watcher then
enables `PLAY`, which renders white in the default theme.

PS2 discs are recognized through a `BOOT2` entry and use the native Linux
PCSX2 runtime `playstation2-1.0.kzr`. The virtual KZI passes `/dev/sr0` to
PCSX2's physical-drive mode, so neither DVD nor CD games are copied internally.
The runtime remains disabled until a user-supplied PS2 BIOS is placed in
`/var/kazeta/firmware/ps2` (also exposed as `firmware/ps2` over FTP).

Additional recognition and runtime mappings:

| Disc or carrier | Runtime | Firmware |
| --- | --- | --- |
| GameCube backup DVD, `.gcm`, or `.rvz` carrier | `dolphin-1.0` | None |
| Wii backup DVD or `.wbfs` carrier | `dolphin-1.0` | None |
| Dreamcast MIL-CD backup, `.cdi`, or `.gdi` carrier | `dreamcast-1.0` | Optional Flycast BIOS in `firmware/dreamcast` |
| Sega CD mixed-mode backup | `segacd-1.0` | `bios_CD_U.bin`, `bios_CD_E.bin`, or `bios_CD_J.bin` in `firmware/segacd` |
| Sega Saturn mixed-mode backup | `saturn-1.0` | `sega_101.bin` or `mpr-17933.bin` in `firmware/saturn` |
| Explicitly identified PC Engine/TurboGrafx CD backup | `pcengine-1.0` | `syscard3.pce` in `firmware/pcengine` |

Original GameCube and Wii optical discs are physically unreadable in most PC
DVD drives. Direct launch therefore works with burned backups and with original
discs only when Linux and Dolphin support the installed drive model. Dreamcast
GD-ROM originals likewise require specialized hardware; standard drives can
test MIL-CD-compatible backups.

## Single-ROM USB drives and data discs

`playfusion-loose-rom-helper` watches `/run/media` for newly mounted media. If
one mounted USB drive or data disc contains exactly one recognizable game
descriptor and no existing KZI/KZP, it creates a small read-only virtual cart
at `/run/media/playfusion-loose-rom`. The ROM remains on the source media; the
virtual cart contains metadata, a runtime mapping, an icon, and a link to the
original file.

The cart ID has the form
`loose-<platform>-<normalized-filename>-<filename-hash>`. It does not contain
the drive label, mount path, file timestamps, or content hash. Reinserting the
same filename from a differently labeled USB drive therefore reuses the same
save directory. Renaming the ROM deliberately creates a separate save ID.

Recognized formats include:

- NES, SNES, Game Boy/Color/Advance, Nintendo 64, Nintendo DS and 3DS
- Master System, Game Gear, Mega Drive/Genesis, 32X, raw PS1 CUE/BIN pairs,
  and strongly identified Sega CD/Saturn/Dreamcast BIN or CUE images
- Atari 2600/7800, Lynx and Jaguar
- Commodore 64 and Amiga disk images
- PSP CSO/ISO, PS2 ISO, Wii/GameCube images, Wii U WUX/WUD, CDI and GDI
- ZIP archives containing exactly one recognizable cartridge ROM

Generic `.bin` files are accepted only when a strong embedded signature
identifies the system. Headerless BIN files remain disabled because size and
extension guesses can select the wrong emulator. Media containing two or more
recognized ROMs is also left untouched; those games should use normal
KZI folders or the internal gallery.
