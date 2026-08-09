# PlayFusion changelog

This file records user-visible changes in every public PlayFusion release.
Unresolved problems are tracked separately in
[`docs/KNOWN-ISSUES.md`](docs/KNOWN-ISSUES.md).

## 1.0.3 - 2026-08-03

### Added

- Added cumulative, signed internet and offline updates for PlayFusion
  1.0.0, 1.0.1, and 1.0.2, with transactional backups, automatic rollback,
  and a next-boot health check.
- Added the Runtime Manager backed by the PlayFusion-Runtimes release channel,
  allowing the Lite installer to download runtimes after installation.
- Rebuilt Theme Management as a controller-friendly gallery with downloaded
  image or video-frame previews, PlayFusion and Kazeta+ theme compatibility,
  apply/reset actions, and support for theme-specific profile-badge placement.
- Added a theme-driven projectM home background, optional MP3 background
  player, LB/RB track switching, and a temporary now-playing notification.
- Added OS-only soft reboot and global game-exit/controller hotkeys, including
  RetroArch menu access.
- Added separate Full and Lite public installers. The Full image contains all
  39 release runtimes; the Lite image retains Runtime Manager and downloads
  only the runtimes selected by the user.

### Fixed

- Rebuilt the incremental 1.0.3 update with Linux LF line endings. The
  original Windows-staged asset placed CRLF endings in media launchers and
  sudoers rules, causing music error 127, DVD failures, failed privileged game
  mounts, and broken reboot actions on systems upgraded from 1.0.2.
- Added release gates that reject CR/CRLF Linux payload text, parse every
  shipped shell launcher, validate sudoers rules, and exercise theme archive
  installation before an update can be signed.
- Made the 1.0.3 post-install repair executable ownership and modes, sudoers
  permissions, linker configuration, and required persistent services.
- Restored the lightweight native Digital Jukebox cabinet visualizer while
  keeping projectM for fullscreen visualization.
- Made the Data screen and active-profile badge follow the selected theme.
- Preserved profiles, saves, controller layouts, themes, audio selection,
  background selection, games, firmware imports, and media across updates.
- Hardened the public installer so its own USB device can never be offered as
  an installation target and disks smaller than 32 GB are excluded.
- Fixed the 1.0 legacy ZIP bridge to resolve the correct versioned update
  folder and package name.
- Added release gates that reject BIOS files, encryption keys, private signing
  keys, personal profiles, saves, and firmware from public images and updates.

See [`docs/RELEASE-1.0.3.md`](docs/RELEASE-1.0.3.md) for installation and
update-test notes.

## 1.0.2 - 2026-08-02 (testing release)

### Fixed

- Fixed legacy 1.0 update installation when sudo executes the bridge through
  `/dev/fd`, and replaced the running BIOS atomically to avoid `ETXTBSY`.
- Fixed false automatic rollback by moving the UI health marker from
  root-owned `/run` to the gamer-owned XDG runtime directory.
- Restored the hardware-tested PS1 optical helper, title database, udev rules,
  and PlayStation 1.01 runtime in clean installer deployments.
- Bundled the complete optimized Xbox and Xbox 2.0 theme archives instead of
  shipping only an Xbox 2.0 background.
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
