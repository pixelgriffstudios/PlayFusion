# PlayFusion 1.0.1

PlayFusion 1.0.1 is a maintenance release for clean public installations.

## Primary fix

Settings, profiles, themes, and other Kazeta+ application data now use a
persistent writable directory under `/var/kazeta`. The installer validates the
link, target ownership, save-directory ownership, clean factory library, and
deployment state before reporting success.

## Real-hardware verification

- Clean installation completed successfully.
- The installed internal drive booted without the installer USB.
- Theme and interface settings persisted across multiple reboots.
- Additional profiles could be created and survived reboot.
- Movie playback and internal movie copying worked.
- HDMI/DisplayPort audio worked after initial output selection.

## Known issues

MP3 and Digital Jukebox playback can begin briefly and then fail with exit code
4 on some systems. A controller mapping can send MPV an unintended `quit 4`
command at startup; this is targeted for 1.0.2 and is not a projectM or codec
failure.

Larger removable-media libraries can require a second insertion before every
artwork download appears. Detection and playback are unaffected.

## Download and reconstruction

Download all ten numbered IMG parts plus the rebuild script for your operating
system. Keep them together, run the rebuild script, and flash the reconstructed
`PlayFusion-1.0.1-Public-Installer.img` with BalenaEtcher or another raw-disk
imaging tool.

Expected reconstructed image:

```text
Bytes:   9777446912
SHA-256: 66EB181B7090A605BCCDEF0448DB115C2D9AF9B86E8951DFC51B731AF388505A
```
