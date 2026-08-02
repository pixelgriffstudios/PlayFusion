# PlayFusion changelog

This file records user-visible changes in every public PlayFusion release.
Unresolved problems are tracked separately in
[`docs/KNOWN-ISSUES.md`](docs/KNOWN-ISSUES.md).

## 1.0.1 - 2026-08-01

### Fixed

- Fixed settings resetting after reboot on clean public installations.
- Moved the complete `~/.local/share/kazeta-plus` user-data directory to
  persistent writable storage under `/var/kazeta/user-data/kazeta-plus`.
- Preserved the original application path with a validated directory symlink.
- Preserved themes, backgrounds, fonts, screensaver choices, profile metadata,
  and other PlayFusion user settings across reboot and shutdown.
- Normalized ownership of factory save, profile, state, cache, and controller
  directories so the first launch can create writable data.
- Added installer validation for the persistent-settings link, target owner,
  clean factory library, and factory save-directory ownership.
- Kept the installed deployment sealed read-only after the writable data path
  is initialized.
- Generated unique SSH host keys for each installation.

### Verified on real hardware

- Clean installation and internal-drive boot.
- Settings persistence across multiple reboots.
- Additional profile creation and profile persistence.
- Movie launch and internal movie copying.
- HDMI/DisplayPort audio selection after initial configuration.

### Known limitation

- MP3 and Digital Jukebox playback can fail with exit code 4 on some systems.
  This does not affect game, profile, save, installer, or movie functionality.

## 1.0.0 - 2026-07-31

Initial public PlayFusion release, including:

- PlayFusion television interface and branding.
- Internal Games library and Game Manager.
- Automatic single-ROM and Multi-ROM removable-media browsing.
- Movies, TV shows, music, Digital Jukebox, and projectM visualizations.
- Default profile plus three optional named profiles with separate saves.
- Internal HDD/SSD expansion storage.
- BIOS Manager and removable-media firmware importer.
- Controller normalization and per-system/per-game layouts.
- Optical-disc support and PS1/PS2 disc handling.
- FTP, SSH, and SFTP maintenance access.
- Native themes, animated backgrounds, and custom screensavers.
- 38 bundled runtimes.
- Hell on Rails and PlayFusion Arcade factory games.
- 30 Years factory music album.
