# PlayFusion 1.0.3

PlayFusion 1.0.3 is a cumulative update for PlayFusion 1.0.0, 1.0.1, and
1.0.2. It is available as a signed in-system update, an older-system bridge
ZIP, a Full installer with all 39 runtimes, and a Lite installer with Runtime
Manager.

## Updating an installed system

Open **Extras > Check for Updates**. PlayFusion downloads the matching `.pfu`,
`.sha256`, and `.sig` assets, verifies the SHA-256 checksum and PlayFusion
signature, backs up every system file it will change, applies the update, and
reboots. A boot-health service automatically restores the previous system
files if the updated menu does not start successfully.

The update does not replace or delete games, saves, profiles, controller
layouts, firmware imports, media, active themes, background selection, or
audio settings.

PlayFusion 1.0.0 uses the release asset named
`PlayFusion-legacy-update-v1.0.3.zip`. The bridge verifies the same signed
package before installing it; it is not an unsigned alternate update.

## Clean installation

- **Full installer:** includes all 39 release runtimes.
- **Lite installer:** includes Runtime Manager and downloads only selected
  runtimes after installation.

Write the IMG to USB with Balena Etcher, boot it in UEFI mode with Secure Boot
disabled, and select **Erase disk and install** only after confirming the exact
internal target disk. The installer hides its own USB and refuses targets
smaller than 32 GB.

Neither installer contains console BIOS files, encryption keys, user saves, or
personal profiles. Import legally obtained firmware with BIOS Manager after
installation.

## Update test checklist

After the first reboot, confirm:

1. The About screen reports PlayFusion 1.0.3.
2. The previous profile, avatar, saves, games, themes, and controller settings
   remain present.
3. The selected HDMI/DisplayPort audio output remains selected.
4. Internal and removable-media games still launch and exit normally.
5. Runtime Manager and Theme Management open successfully.
6. MP3 background playback, Digital Jukebox cabinet visuals, and fullscreen
   projectM work.
7. **Power > Soft Reboot** returns to the menu without rebooting the computer.

If the menu cannot report healthy after the update, PlayFusion automatically
rolls back the changed system files on the next boot.
