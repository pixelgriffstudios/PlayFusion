# Current known issues

These issues apply to the PlayFusion 1.0.3 release unless noted otherwise.
Confirmed fixes are recorded in `CHANGELOG.md`.

## Experimental Android support

The Waydroid runtime and Android controller tools are experimental and are not
release-qualified in PlayFusion 1.0.3. No APKs are installed in the factory
library. Some APKs can return to the PlayFusion menu, lack visible touch input,
or fail on kernels and graphics drivers that do not satisfy Waydroid's
requirements.

Status: development is paused. Android support must not be treated as a stable
feature when qualifying the 1.0.3 installer.

## Artwork completion on larger removable-media libraries

When removable media contains many items, the first scan can populate only
part of the artwork cache. In one real-hardware test with 16 files, three covers
appeared on the first insertion and the remainder appeared after the media was
inserted a second time. Detection and playback still worked.

Status: artwork queue completion and visible progress are targeted for
PlayFusion 1.0.4.

## Initial HDMI/DisplayPort audio selection

The first boot can select an internal speaker instead of the connected
HDMI/DisplayPort display. Selecting the correct output in Settings and rebooting
can make the selection persist, but profile creation and early reboot timing
still need additional testing.

Status: remains hardware-dependent; the selected output is persisted once the
PipeWire device is available during boot.

## Many internal expansion drives

The storage backend supports multiple internal HDDs, SSDs, NVMe devices, and
eMMC devices, with each library mounted by filesystem UUID. The current Storage
Manager draws only the rows that fit on screen. With five or six expansion
drives, later entries can be recognized by the backend but hidden at 720p.

Status: scrolling is planned for a later release.

## Virtual machines

Kazeta and Kazeta+ are not designed or supported as virtual-machine guests.
The PlayFusion installer may run under QEMU, but the installed system can stall
on virtual networking, graphics, Gamescope, controller, or acceleration setup.
Use supported real hardware for release qualification.
