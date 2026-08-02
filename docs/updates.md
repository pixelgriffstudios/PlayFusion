# Safe PlayFusion updates

PlayFusion 1.0.2 is the baseline for internet updates. Open **Extras > Check
for Updates** to query only the official `pixelgriffstudios/PlayFusion` GitHub
release channel.

Updates are cumulative by default. A console on 1.0.2 can install a later
1.0.4, 2.0.0, or newer package directly when that package declares 1.0.2 as
its minimum version. A release can require an intermediate migration only when
its signed manifest explicitly raises the minimum version.

Before changing a system file, PlayFusion requires all three exact release
assets:

```text
PlayFusion-update-vX.Y.Z.pfu
PlayFusion-update-vX.Y.Z.pfu.sha256
PlayFusion-update-vX.Y.Z.pfu.sig
```

The updater verifies the SHA-256 checksum and a PlayFusion-owned public-key
signature. It rejects Kazeta+, arbitrary ZIP, installer-image, incomplete,
corrupted, downgraded, wrong-product, unsafe-path, and unsigned packages.

The update helper never writes to `/var/kazeta`, home directories, games,
saves, profiles, firmware imports, themes, or media libraries. Every changed or
deleted system file is backed up first. A failed apply or post-install step is
rolled back immediately. After reboot, a system service waits for the
PlayFusion menu to report healthy; if that never happens, the prior files are
restored automatically and the machine reboots.

The private release-signing key is not stored in Git, in the installer image,
or on the PlayFusion console. Only its public verification key ships with the
OS.

## Offline update USB

Place the matching `.pfu`, `.sha256`, and `.sig` files in an `updates/` folder
on USB or SD. The normal **Check for Updates** screen detects the complete
signed set. Do not rename any of the three files.
