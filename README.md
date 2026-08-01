# PlayFusion

PlayFusion is a controller-friendly gaming and media operating system based on
[Kazeta](https://github.com/kazetaos/kazeta) and
[Kazeta+](https://github.com/the-outcaster/kazeta-plus).

It keeps Kazeta's insert-media-and-play model while adding an internal game
library, automatic loose-ROM media, movies and music, a digital jukebox,
profiles, storage expansion, firmware management, additional emulator
runtimes, and a fully reworked television-friendly interface.

## Download

Download the current public installer from the
[PlayFusion releases](https://github.com/pixelgriffstudios/PlayFusion/releases)
page. The installer is distributed as numbered parts because the complete raw
disk image is larger than GitHub's per-asset limit. Download every part and use
the included Windows or Linux rebuild script before flashing the IMG.

## Major features

- 38 bundled Kazeta runtimes
- PlayFusion Arcade and Hell on Rails factory games
- System-organized Internal Games gallery and Game Manager
- Automatic single-ROM and multi-ROM USB/SD/data-disc support
- PS1/PS2 and supported backup-disc identification
- Movies, TV shows, music, DVD playback, and Digital Jukebox
- projectM cabinet and fullscreen audio visualizations
- Default plus three optional named profiles with separate saves
- Additional internal HDD/SSD game libraries
- BIOS/firmware inventory and USB importer
- Controller normalization and per-system/per-game layouts
- FTP, SSH, and SFTP maintenance access
- Native themes, animated backgrounds, and custom screensavers

## Source layout

- `bios/` - Rust television interface and PlayFusion visual assets
- `rootfs/` - files and services installed into the system image
- `playfusion-arcade/` - source for the bundled arcade collection
- `optical-stream/` - optical-disc streaming source
- `tools/` - installer, patching, runtime, and release tooling
- `docs/` - feature, runtime, firmware, and internal-library documentation
- `manifest` - Arch Linux package and service manifest

## Building

The image builder targets Arch Linux and expects the Kazeta/Kazeta+ build
environment. Runtime `.kzr` files are staged outside Git:

```bash
sudo env \
  RUNTIME_BUNDLE_DIR=/absolute/path/to/release-runtimes \
  ./build-image.sh
```

The public build deliberately excludes private firmware. For a personal build
using legally dumped firmware, follow `docs/private-build.md` and keep the
private staging directory outside Git.

## Firmware and copyrighted content

Do not submit console BIOS dumps, encryption keys, commercial ROMs/disc images,
private signing keys, user profiles, or save data. PlayFusion includes a BIOS
Manager so owners can import legally obtained files after installation.

## Credits

- Original Kazeta concept and implementation: the Kazeta project
- Kazeta+ enhancements: The "Overly Complex" Kazeta+ Guy / The Outcaster
- PlayFusion fork and development: Jason Griffith / PixelGriff Studios

See [NOTICE.md](NOTICE.md) and [LICENSE](LICENSE).

