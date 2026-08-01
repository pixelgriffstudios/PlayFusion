# PlayFusion Runtime Suite

Each system is packaged as a separate Kazeta `.kzr` runtime. A game's `cart.kzi`
selects exactly one runtime with `Runtime=<name>`, so a `.gba` file can never be
mistaken for an arcade ROM or a PC executable.

## Runtime map

| Runtime | Emulator/core | Typical content | Firmware |
|---|---|---|---|
| `psp-1.0` | RetroArch + PPSSPP | `.iso`, `.cso`, `.pbp`, `.chd` | No BIOS; PPSSPP assets are included |
| `gameboy-1.0` | RetroArch + mGBA | `.gb` | Optional Game Boy BIOS |
| `gameboycolor-1.0` | RetroArch + mGBA | `.gbc` | Optional Game Boy Color BIOS |
| `gameboyadvance-1.0` | RetroArch + mGBA | `.gba` | Optional Game Boy Advance BIOS |
| `nintendods-1.0` | RetroArch + melonDS DS | `.nds`, `.dsi`, `.ids` | Optional for DS; required for DSi mode |
| `arcade-fbneo-1.0` | RetroArch + FinalBurn Neo | Matching FBNeo ROM sets | Some boards need matching BIOS ROM sets |
| `arcade-mame-1.0` | RetroArch + current MAME | Matching current-MAME ROM sets | Machine-dependent BIOS/device ROM sets |
| `mastersystem-1.0` | RetroArch + Genesis Plus GX | Master System ROMs | None |
| `gamegear-1.0` | RetroArch + Genesis Plus GX | Game Gear ROMs | None |
| `sega32x-1.0` | RetroArch + PicoDrive | 32X ROMs | None for 32X cartridge games |
| `atari2600-1.0` | RetroArch + Stella | Atari 2600 ROMs | None |
| `atari7800-1.0` | RetroArch + ProSystem | `.a78`, `.bin` | Optional `7800 BIOS (U).rom` |
| `atarilynx-1.0` | RetroArch + Handy | `.lnx` | Required `lynxboot.img` |
| `dosbox-1.0` | RetroArch + DOSBox Pure | DOS archives/images/executables | None |
| `scummvm-1.0` | RetroArch + ScummVM | Supported adventure-game data | No system BIOS; original game data required |
| `amiga-1.0` | RetroArch + PUAE | ADF/HDF/WHDLoad/CD images | AROS fallback included by core; real Kickstart strongly recommended |
| `commodore64-1.0` | RetroArch + VICE x64sc | C64 disks/tapes/cartridges | Standard core works without user BIOS; optional JiffyDOS ROMs |
| `jaguar-1.0` | RetroArch + Virtual Jaguar | Jaguar ROMs | None |
| `nintendo3ds-1.0` | RetroArch + Azahar | Decrypted 3DS images | No traditional BIOS; encrypted content needs dumped AES keys |
| `playstationvita-1.0` | Vita3K AppImage | Decrypted app folder, `.vpk`, `.zip` | Official Vita firmware and font package required |
| `xbox-1.0` | xemu AppImage | Xbox-compatible XISO | MCPX boot ROM, compatible BIOS, and Xbox HDD image required |

All RetroArch cores use controller-native RetroPad mapping. AntiMicroX is not
started for any of these emulator runtimes.

## Firmware locations

User-supplied RetroArch firmware is shared from:

```text
/var/kazeta/firmware/retroarch/
```

The runtime launcher links that tree into RetroArch's `system` directory at
launch. Preserve required subdirectories such as `vice/`.

Original Xbox files go in:

```text
/var/kazeta/firmware/xbox/
  mcpx_1.0.bin
  Complex_4627.bin
  xbox_hdd.qcow2
```

xemu's official documentation lists MCPX MD5
`d49c52a4102f6df7bcf8d0617ac475ed`. A default EEPROM is generated per game.
The shared HDD stays writable so Xbox saves persist across games.

Vita3K stores installed firmware, applications, shaders, and saves inside the
game's persistent Kazeta overlay. Sony firmware is not redistributed.

## Specific optional/required RetroArch files

- mGBA optional: `gba_bios.bin`, `gb_bios.bin`, `gbc_bios.bin`,
  `sgb_bios.bin`.
- melonDS DS optional: `bios7.bin`, `bios9.bin`, `firmware.bin`.
- melonDS DSi mode required: `dsi_bios7.bin`, `dsi_bios9.bin`,
  `dsi_firmware.bin`, `dsi_nand.bin`.
- Handy required: `lynxboot.img`, MD5
  `fcd403db69f54290b51035d82f835e7b`.
- ProSystem optional: `7800 BIOS (U).rom`, MD5
  `0763f1ffb006ddbe32e52d497ee848ae`.
- PUAE: place legally dumped Kickstart ROMs in the shared RetroArch firmware
  directory. The core can use its AROS fallback, but compatibility is lower.
- FinalBurn Neo/MAME: keep BIOS ZIPs from the exact ROM-set version with the
  game ROM or in the appropriate system directory; do not unpack them.
- Azahar: place legally dumped `aes_keys.txt` and required 3DS system archives
  in the layout documented by Azahar when using encrypted titles or features
  that need system data.

Firmware, keys, commercial ROMs, and copyrighted game content are intentionally
not included in the release runtime bundle.

## BIOS Files menu and USB import

The public PlayFusion image includes every emulator runtime, but does not
redistribute copyrighted console firmware or encryption keys. Open
**Extras -> BIOS Files** to see a controller-friendly inventory. Required
missing files are red, installed files are green, and optional/manual items are
yellow or purple.

To import firmware, copy legally obtained files to any folder on a USB drive,
insert and mount the drive, then choose **Scan USB**. The scanner searches up to
six folders deep, checks recognized names and expected file sizes, and places
valid files in the correct `/var/kazeta/firmware` subdirectory. It does not
delete anything from the USB drive. Unknown files are ignored, and an existing
destination is never overwritten; a different file at the same destination is
reported as a conflict.

Recognized imports include:

- PlayStation: `scph5501.bin`; PlayStation 2: 4 MiB `scph#####.bin`
- Sega CD: `bios_CD_U.bin`, `bios_CD_E.bin`, `bios_CD_J.bin`
- Saturn: `sega_101.bin`, `mpr-17933.bin`
- PC Engine CD: `syscard3.pce`
- Atari Lynx: `lynxboot.img`; Atari 7800: `7800 BIOS (U).rom`
- Game Boy family: `gb_bios.bin`, `gbc_bios.bin`, `gba_bios.bin`,
  `sgb_bios.bin`
- Nintendo DS/DSi: `bios7.bin`, `bios9.bin`, `firmware.bin`,
  `dsi_bios7.bin`, `dsi_bios9.bin`, `dsi_firmware.bin`, `dsi_nand.bin`
- Amiga: common `kick*.rom`, `kick*.bin`, and `kick*.a500` dumps
- Dreamcast: `dc_boot.bin`, `dc_flash.bin`
- Wii U: a Cemu-format `keys.txt`; Nintendo 3DS: `aes_keys.txt`
- Original Xbox: `mcpx_1.0.bin`, `Complex_4627.bin` or
  `Complex_4627v1.03.bin`, and `xbox_hdd.qcow2`
- Vita firmware packages: `PSVUPDAT.PUP` or `PSP2UPDAT.PUP` are collected,
  but must still be installed through Vita3K.

Arcade BIOS/device ZIPs remain paired with the exact MAME or FinalBurn Neo ROM
set and are not moved by the generic scanner.

## PC keyboard-only game mapping

AntiMicroX is installed as an optional system feature, not as an emulator
runtime. It only starts when all of these are true:

1. The selected runtime name begins with `windows` or `linux`.
2. A per-game PlayFusion profile has been enabled, or the KZI explicitly sets
   `KeyboardProfile=<relative-profile-file>`.
3. The AntiMicroX AppImage is installed.

Profiles created from **Extras → PC Controller Profiles** are stored under:

```text
/var/kazeta/controller-profiles/<game-id>/
```

This prevents keyboard emulation from interfering with Wii, PSP, arcade, Xbox,
or any other emulator that already supports controllers.

The default PC profile maps the Xbox Guide button to the on-screen keyboard.
Press Guide again to hide it. Closing the PC game also closes the keyboard.

## Primary documentation

- Libretro BIOS guide: https://docs.libretro.com/guides/bios/
- PPSSPP core: https://docs.libretro.com/library/ppsspp/
- melonDS DS core: https://docs.libretro.com/library/melonds_ds/
- Handy core: https://docs.libretro.com/library/handy/
- xemu required files: https://xemu.app/docs/required-files/
- xemu CLI: https://xemu.app/docs/cli/
- Vita3K quickstart: https://vita3k.org/quickstart.html
- AntiMicroX: https://github.com/AntiMicroX/antimicrox
