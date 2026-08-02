# PlayFusion 1.0.2 testing release

PlayFusion 1.0.2 is the final planned full-image baseline. Later compatible
versions can be delivered as cumulative, signed `.pfu` updates from **Extras >
Check for Updates** without installing every intermediate version.

## Release status

Version 1.0.2 is published as a testing/pre-release until its clean installer,
first boot, settings persistence, game launch, media playback, and updater are
confirmed on real hardware. PlayFusion 1.0.1 remains available as the rollback
installer during this qualification period.

## Factory contents

- 39 bundled runtimes
- Hell on Rails and PlayFusion Arcade
- 30 Years music album
- one clean Default profile and no personal saves
- Retro Laser Grid factory background at 1280x720
- optimized Xbox Original and native Xbox 2.0 optional themes
- no factory movies or Android APKs

## Major corrections

- Persistent MP3 and Digital Jukebox state no longer exits with code 4.
- The active profile name/avatar is visible on the home screen.
- Theme boot animations replace the default cartridge animation and use the
  stable Vega-safe movie playback path.
- Clean installs no longer fail during Btrfs deployment when Android test
  libraries carry compression attributes that conflict with nodatacow.
- Signed cumulative internet/offline updates validate hashes and signatures,
  protect user data, keep transactional backups, and roll back automatically
  if the UI fails its next-boot health check.

See the full [changelog](../CHANGELOG.md), [theme guide](themes.md), [update
guide](updates.md), and [known issues](KNOWN-ISSUES.md).
