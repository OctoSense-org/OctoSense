# OnePlus 6 framebuffer cleanup and display blackout

The recurrent blackout on the September 19 Octosense ROM leaves Android and
Octos alive while the physical panel stops displaying frames. Rebooting
recovers the display but does not correct the fault.

## Evidence and limits

The September 23 diagnostic trace shows:

- At 15:38:37, wake enables the panel. A kernel worker then tries to remove a
  physical plane while a paired virtual plane is still present. SDE rejects
  that transaction with `-EINVAL`; the CRTC is still enabled and active.
- At 15:38:38, the panel is disabled before the next Android power-off request.
  Subsequent composer transactions see a disabled CRTC and fail validation,
  including single-plane composition. Their requested state sets a mode but
  leaves the CRTC inactive.
- A later ON request is skipped because the composer's cached state is already
  ON. The display remains unusable until recovery.

The paired-plane rejection also occurs without a sustained blackout. It is
not, by itself, proof of what disabled the CRTC. The zero adjusted mode in a
rejected transaction does not prove userspace submitted a zero-sized mode:
validation may fail before mode fixup.

Before this change, the kernel checkout matched LineageOS commit
`2e921a892c03b8a17b4d82e9b24c2b3aa775c870`. The **actual CAF** display tree,
`hardware/qcom-caf/sdm845/display`, matched `601faec`; the separate
`hardware/qcom/sdm845/display` tree is not the relevant composer source.
Octosense's device-tree additions include its products and SELinux policy.

This is a source comparison, not an A/B test against a fresh official LineageOS
installation. It does not establish that stock LineageOS reproduces the bug,
or identify which Octosense lifecycle/rendering change first exposed it.

## Change

The kernel's legacy `drm_framebuffer_remove()` unconditionally force-disables
the CRTC when a primary framebuffer is removed. Qualcomm's composer removes
framebuffers when layer caches are evicted or destroyed, and its normal frame
setup does not restore CRTC active state and connector routing after an
unexpected kernel-side disable.

`patches/kernel/oneplus6-atomic-rmfb.patch` backports atomic framebuffer removal
from [upstream 1592364de391](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/commit/?id=1592364de3912dad264262f4bcc61552984c9523),
with the retry structure used in
[Linux v4.19](https://github.com/torvalds/linux/blob/v4.19/drivers/gpu/drm/drm_framebuffer.c).
Atomic drivers first detach matching planes in one transaction while preserving
the CRTC mode, active state, and connector routing. The upstream fallback to
disable a primary plane's CRTC remains if the driver rejects that state.
Non-atomic drivers retain their previous path.

The backport preserves two contracts specific to this 4.9 tree: successful
commit transfers ownership of the atomic state to the driver, and legacy
`plane->fb` mirrors hold references that need updating under the modeset locks.
Deadlock retry clears the failed state before reacquiring locks. Rejected
transactions remain rejected; the patch does not bypass SDE validation.

The helper stays inside `drm_atomic.c`, with an internal declaration, to avoid
changing exported symbol checksums merely by exposing atomic type definitions
to the framebuffer translation unit. ROM staging applies the patch through
`scripts/apply-to-tree.sh`; the applicator verifies an already-applied patch
and fails on incompatible source.

## Validation

`python3 -m unittest discover -s tests -p test_atomic_rmfb.py -v` compiles the
actual patch's C function with AddressSanitizer and UndefinedBehaviorSanitizer.
Sixteen fault-injection cases cover primary/overlay/shared-buffer removal,
unused buffers, the required-primary fallback, invalid paired-plane removal,
allocation and commit errors, partial preparation failure, and deadlocks during
locking, preparation, and commit. These validate transaction and ownership
contracts; they do not simulate the physical display hardware.

Before the patch, eight Settings/Recents/Home/sleep/wake cycles completed without
`BAD_DISPLAY` or reboot, but produced 14 paired-plane cleanup errors. This short
baseline did not reproduce the persistent blackout.

Device validation and image provenance are recorded under `out/display-fix`.
Raw phone logs remain local and private. Physical screen visibility must be
confirmed separately from Android reporting its display as ON.

The candidate image `boot-display-fix1.img` was temporarily booted on slot B;
no partition was flashed. Its SHA-256 is
`4ac95fb629c458b3d504384c08e385e1cbd22bd3d8267e44cbef28a56201e364`.
The kernel configuration, all 11,860 exported vmlinux symbol versions, ramdisk,
appended device trees, and boot header arguments match the installed image.
The runtime marker and kernel build identity were verified, and the user
confirmed that the physical screen was visible.

Sixteen Settings/Recents/Home/sleep/wake cycles and eight browser video/Home/
sleep/wake cycles completed with zero `BAD_DISPLAY` entries or unexpected
reboots. The browser used the Qualcomm AVC decoder with a small muted Chromium
test video. The temporary video server and its USB reverse mapping were
removed afterward. Both Octos services and the existing USB network proxy were
restored.

**The intermittent blackout is not yet proven resolved.** The runs still
produced 76 paired-plane rejection log entries (the new atomic path retries
`-EINVAL`, so raw counts are not directly comparable with the old path), and
did not exercise primary-plane removal. The current result establishes a
bootable, compatible backport and short-run stability, not a confirmed causal
fix for every blackout. A normal reboot returns to the installed kernel.

## Normal ROM build integration

The ROM staging entry point now applies the production backport on every
build-tree setup. The patch, idempotent applicator and fault-injection harness
are checked into this repository; the additional diagnostic trace patches and
temporary boot-image repackaging are not part of this integration.

The full ROM rebuild uses the existing OnePlus 6 build tree and prebuilt Home
and Bridge APKs, with the display cleanup backport in its normal kernel build.
It does not roll in the separate, uncommitted Mail or Octos application work.
Artifact verification must confirm that the new boot image contains the
backported helper and that the OTA names the new build and device.

`scripts/build-rom.sh` now propagates compiler failures through its log
pipeline and refuses to export a ZIP older than the current build's start
marker. A failed build cannot leave a new completion marker or silently
substitute the previous ROM. Regression tests exercise both failure cases,
including the module-build pipeline.

Building the ROM does not install it on the phone. The earlier temporary-boot
results remain the available hardware evidence until this ROM is flashed and
its display behavior is retested.
