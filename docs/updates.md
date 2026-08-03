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

The update helper never deletes games, saves, profiles, firmware imports,
user-created themes, or media libraries. Signed releases may add or repair an
official theme by merging its files into the matching theme folder; unrelated
user files remain intact. Every changed or deleted system file is backed up
first. A failed apply or post-install step is rolled back immediately. After
reboot, a system service waits for the PlayFusion menu to report healthy in the
gamer-owned XDG runtime directory; if that never happens, the prior files are
restored automatically and the machine reboots.

The private release-signing key is not stored in Git, in the installer image,
or on the PlayFusion console. Only its public verification key ships with the
OS.

## PlayFusion 1.0 compatibility

The original PlayFusion 1.0 menu understands only the older Kazeta+ ZIP
protocol. While 1.0 remains eligible for direct updates, every compatible
GitHub release must also publish this asset:

```text
PlayFusion-legacy-update-vX.Y.Z.zip
```

That ZIP is a signed-update bridge, not an unsigned replacement payload. It
contains the same `.pfu`, checksum, and signature, verifies them before making
changes, installs the protected PlayFusion updater, and then delegates the
actual update to it. Future releases must publish both the signed `.pfu` trio
and the legacy ZIP whenever their manifest still permits upgrading from 1.0.0.

## Offline update USB

Place the matching `.pfu`, `.sha256`, and `.sig` files in an `updates/` folder
on USB or SD. The normal **Check for Updates** screen detects the complete
signed set. Do not rename any of the three files.
