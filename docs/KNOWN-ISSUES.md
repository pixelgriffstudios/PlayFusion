# Current known issues

These issues apply to the PlayFusion 1.0.1 public installer unless noted
otherwise. Confirmed fixes will be recorded in `CHANGELOG.md` when released.

## Experimental Android support

The Waydroid runtime and Android controller tools are experimental and are not
release-qualified in PlayFusion 1.0.2. No APKs are installed in the factory
library. Some APKs can return to the PlayFusion menu, lack visible touch input,
or fail on kernels and graphics drivers that do not satisfy Waydroid's
requirements.

Status: development is paused. Android support must not be treated as a stable
feature when qualifying the 1.0.2 installer.

## MP3 and Digital Jukebox playback

On some hardware, attempting to play an MP3 directly or through the Digital
Jukebox reports exit code 4. The player log confirms that MPV successfully
opens the audio through PipeWire, then receives an explicit `quit 4` command
immediately after the Xbox controller is detected. This points to the current
MPV gamepad action mapping, not projectM, the codec, or a damaged audio file.
Movie playback and game audio are not affected.

Status: fixed in the in-development PlayFusion 1.0.2 source and verified on
real hardware. The published PlayFusion 1.0.1 image is still affected.

## Artwork completion on larger removable-media libraries

When removable media contains many items, the first scan can populate only
part of the artwork cache. In one real-hardware test with 16 files, three covers
appeared on the first insertion and the remainder appeared after the media was
inserted a second time. Detection and playback still worked.

Status: artwork queue completion and visible progress are targeted for
PlayFusion 1.0.2.

## Initial HDMI/DisplayPort audio selection

The first boot can select an internal speaker instead of the connected
HDMI/DisplayPort display. Selecting the correct output in Settings and rebooting
can make the selection persist, but profile creation and early reboot timing
still need additional testing.

Status: under investigation for PlayFusion 1.0.2.

## Many internal expansion drives

The storage backend supports multiple internal HDDs, SSDs, NVMe devices, and
eMMC devices, with each library mounted by filesystem UUID. The current Storage
Manager draws only the rows that fit on screen. With five or six expansion
drives, later entries can be recognized by the backend but hidden at 720p.

Status: scrolling is planned for PlayFusion 1.0.2.

## Virtual machines

Kazeta and Kazeta+ are not designed or supported as virtual-machine guests.
The PlayFusion installer may run under QEMU, but the installed system can stall
on virtual networking, graphics, Gamescope, controller, or acceleration setup.
Use supported real hardware for release qualification.
