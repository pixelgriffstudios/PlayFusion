# Private PlayFusion build

PlayFusion keeps owner-supplied BIOS files, emulator firmware, title keys, and
shader caches under:

```text
/var/kazeta/firmware/
```

Normal upgrades create missing subdirectories but do not delete or replace
files in this tree.

For a personal full image, copy the complete firmware tree into an external
directory that is not committed to Git. The default local staging directory is
`private-firmware/`, which is excluded by `.gitignore`.

Build the personal image with:

```bash
sudo env \
  INCLUDE_PRIVATE_FIRMWARE=1 \
  PRIVATE_FIRMWARE_DIR=/absolute/path/to/private-firmware \
  ./build-image.sh
```

This preserves, among other files:

- `ps1/` and `ps2/` BIOS dumps
- `segacd/`, `saturn/`, and `pcengine/` firmware
- `wiiu/keys.txt` and Wii U shader caches
- `retroarch/` shared firmware
- `xbox/`, `dreamcast/`, and future firmware subdirectories

The build fails instead of silently producing an incomplete private image when
`INCLUDE_PRIVATE_FIRMWARE=1` is set and the selected directory does not exist.

The public build profile is the default. It creates the firmware folders but
does not package console BIOS files or encryption keys. Those files can be
imported later through the firmware FTP directory.

All public and private PlayFusion images include the complete emulator runtime
set. Stage the redistributable `.kzr` files outside Git in `release-runtimes/`
or select another directory:

```bash
sudo env \
  RUNTIME_BUNDLE_DIR=/absolute/path/to/release-runtimes \
  ./build-image.sh
```

The build stops if the runtime directory is missing or empty. Development-only
images may explicitly opt out with `REQUIRE_RUNTIME_BUNDLE=0`; release images
must not use that override.
