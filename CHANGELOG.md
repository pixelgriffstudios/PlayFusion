# PlayFusion changelog

This file records user-visible changes in every public PlayFusion release.
Unresolved problems are tracked separately in
[`docs/KNOWN-ISSUES.md`](docs/KNOWN-ISSUES.md).

## 1.0.2 - 2026-08-02 (testing release)

### Fixed

- Fixed MP3 and Digital Jukebox exit code 4 on read-only public installs.
- Moved projectM configuration, preset links, and font caches to persistent
  writable storage under `/var/kazeta/state`.
- Preserved both fullscreen projectM and cabinet-mode off-screen visualization.
- Verified MPV playback, projectM rendering, HDMI monitor capture, and cabinet
  rendering on real hardware.
- Added the active profile avatar and profile name to the top-right corner of
  the home screen.
- Added left/right placement for the active-profile badge and made its border
  and text follow the selected theme's highlight and font colors.
- Added the optimized original Xbox theme and the native-rendered Xbox 2.0
  theme, both with matching system-folder artwork and an optional validated
  Xbox boot animation.
- Added safe optional per-theme H.264 boot animations with strict limits and
  automatic fallback to the built-in splash.
- Theme boot animations now replace the native cartridge splash; the native
  splash remains the fallback for the Default theme or an invalid theme video.
- Theme video playback now uses the same Vega-safe OpenGL/VA-API-copy path as
  the corrected movie player to prevent flashing and corrupted frames.
- Added a PlayFusion-only update channel with exact signed `.pfu` assets,
  cumulative-version support, protected user data, transactional file backups,
  immediate failure rollback, and a next-boot UI health check.
- Added offline signed updates from an `updates/` folder on USB or SD.
- Updated the public installer to ship all 39 runtimes, both approved Xbox
  themes, a clean 720p Retro Laser Grid factory configuration, and only the
  two approved factory games.
- Removed private Waydroid APK test state from the public deployment and added
  a release gate that rejects `btrfs.compression` attributes incompatible with
  the installer's nodatacow target.

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
