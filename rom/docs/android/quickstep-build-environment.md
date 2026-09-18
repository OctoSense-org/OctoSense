# Quickstep platform build environment

Status: installation approved on 16 September 2026; the pinned base image and build image are installed.
AMD64 userspace execution and platform C/Java/Go compilation pass. The full
1,141-project bulk checkout completed, then the recovered phone manifest was
used to align all 1,145 public project revisions. Sources and build output use
about 135 GB within the approved 400 GB limit. The Soong-only `aapt2` preflight
passes. The emulated APK build was stopped at the user's request to move to a
native x86 Linux host. The replacement host `ubuntu@54.198.120.231` accepted
the supplied `octosense.pem` key on 17 September. Its source sync and 1,145-project
audit pass. The native `aapt2` preflight, upstream Trebuchet APK build and separate
OctoSense adapter build pass. The locally signed native APK passes inspection.
Native Quickstep deployment is separate. See the
[ADR 0001 implementation record](adr-0001-implementation-record.md).

At the start of the experiment, no Linux build host or LineageOS checkout was
available. The local ARM64 Mac has
128 GiB RAM and about 1.3 TiB free disk. Docker is already running Linux ARM64
with 12 CPUs and 32 GiB RAM assigned. Android's supported platform build uses
x86-64 Linux; current documented requirements include 64 GB RAM and 400 GB free
disk. The installed public Android SDK cannot build native Trebuchet Quickstep.
[Android requirements](https://source.android.com/docs/setup/start/requirements).

## Native Linux migration — 17 September 2026

The supplied replacement host has 72 x86-64 logical CPUs, 145,322,120 KiB RAM
and about 1 TB free on its existing root filesystem. Its host OS is Ubuntu 26.04.
The already-approved Ubuntu 24.04 builder image was exported and transferred,
retaining its installed package versions. The root-filesystem archive SHA-256 is
`0d381e1eac6e524509ab0b6b5d724cd83ba98d5a4dbd8e1ddd1e91878825ac4f`;
the server verified it before extraction. No Docker installation or new build
package versions were needed on the server.

Build files live under `/home/ubuntu/octosense-adr0001/`. A dedicated systemd
unit, `octosense-adr0001.service`, runs the Ubuntu userspace in private mount/PID
namespaces, with compilation as UID 1000. It has a 120 GiB memory ceiling,
112 GiB memory pressure threshold, 64-CPU quota and 12-hour deadline. Source
sync uses 16 jobs and the platform builds use 32. Go's analysis settings are
`GOMAXPROCS=16`, `GOGC=100`, `GOMEMLIMIT=48GiB`; the last value is a soft heap
target, not the job's total memory limit. The source/output supervisor retains
the 390 GB stop threshold and 50 GB free-space floor.

The fresh checkout uses the same pinned repo tool, initial manifest revision
and installed-ROM overlay. Sync completed in 923 seconds, using about 123 GB.
All 1,145 project identities/revisions and the one recorded Soong memory-control
patch pass the source audit. The resolved manifest SHA-256 remains
`9d08129ad7be4d2ef24b5596790023f4996aeb37170298a9c2882c21f236ef66`.
The pipeline then runs `aapt2` and the upstream `TrebuchetQuickStep` baseline.
The native `aapt2` preflight completed all 977 actions and passed its executable
probe in 324 seconds, with zero OOM events. The upstream APK build also passes.
The same nonfatal Windows COFF strip diagnostic remains in the retained log.
The first native APK attempt reached 8,652 of 9,226 actions before its supervisor
terminated it on `du` exit status 1. The compiler was active and the cgroup
reported zero OOM events. A post-stop disk sample succeeds; the original monitor
did not preserve `du`'s stderr, so the precise disappearing path is unknown.
The corrected monitor records diagnostics and accepts a valid total only when
all reported errors are missing paths, which compilation can remove during a
directory walk. Other errors still stop the job. The original attempt is retained
under `upstream-build-disk-sampling-stop/`. The cache-preserving retry runs as
`octosense-adr0001-retry.service`, verifies the retained preflight executable hash
and repeats the full source audit before resuming the APK target. The retry
completed its remaining 575 actions in 337 seconds, with a 29,090,709,504-byte
cgroup peak and zero OOM events. The retained upstream APK SHA-256 is
`e2313039cd20832e3d10e8d0256a63510dccc6f84a2ea168d7bf25af2a19c9c7`.
This time excludes work completed by the interrupted first native attempt.
The separate adapter build completed as `octosense-adr0001-native.service`, using
that passing baseline APK. Its pre-build audit passes and permits only the exact staged sources and
the appended OctoSense Blueprint fragment; staging metadata alone is insufficient.
All 46 adapter actions complete in 296 seconds with a 34,753,990,656-byte memory
peak and zero OOM events. The build-signed APK SHA-256 is
`3a7f3a755061f690e653deade4e6c0f96d4a0965659be5e389eb7b5135897959`.
After signing on the Mac with the existing OctoSense development key, its SHA-256
is `fba417a89253cec42274d07077a757d3a565349e9a8f11b19de6c2e9374e0ee0`.
Package/service/metadata/provider/signature checks pass. Both native build units
have exited successfully; the remote sources and cache remain available.
The old Mac volume and failed/stopped build evidence remain preserved.

Local migration records and remote build recipes are under
`~/.local/share/octosense/android-platform-build/native-host/`.
The private SSH key and OctoSense signing key remain on the Mac. Native build
completion and APK inspection now pass. Phone deployment and transition
validation remain separate gates; the concrete module and rollback are in the
[deployment review](quickstep-deployment-review.md).

## Approved local experiment

Use the existing Docker installation to test AMD64 emulation before syncing a
large source tree. Success is not assured: the emulator must run the platform's
Linux prebuilts, and the current Docker memory allocation is below Android's
documented full-build requirement. A native x86-64 Linux host remains the
supported fallback. No cloud account, server rental or Docker restart is part
of this proposal. Docker documents its optional Rosetta acceleration separately;
its availability and settings have not been verified here.
[Docker settings](https://docs.docker.com/desktop/settings-and-maintenance/settings/).

Approved software and locations:

- Official Ubuntu 24.04 `linux/amd64` image, pinned to manifest SHA-256
  `496754492fb28b4d3049432f2ca787449331e23fb14f0dd3fffea86bf5a93eb4`,
  in the existing Docker image cache.
- The Ubuntu build packages listed in
  [Dockerfile](../../android/platform-build/Dockerfile), from Ubuntu's signed
  24.04 repositories. Resolved package versions are recorded in the image at
  `/opt/octosense/packages.tsv`; only the base image is pinned by digest.
- LineageOS 22.2/AOSP sources and their included Linux build tools in a dedicated
  case-sensitive Docker volume, `octosense-lineage22-build`. Allow up to
  400 GB initially; check available space before download. Export manifests,
  logs and APK candidates under
  `~/.local/share/octosense/android-platform-build`.

The pinned base image passed an x86-64 userspace execution check with
`--platform=linux/amd64` before building this Docker context:

```sh
docker build --platform=linux/amd64 \
  -t octosense-lineage22-builder:ubuntu24.04 android/platform-build
```

Verify the compiler, Java/tool generators and a small Soong target before a
Trebuchet build. Start with two build jobs and the existing Docker limits.
Changing Docker's memory/VM settings requires a separate reviewed decision,
because a restart can interrupt other containers. Store source and build output
in the Linux volume, not the Mac's case-insensitive shared filesystem.

The installed builder image ID is
`sha256:de4832e5c4865b364a2cd408f6d0d0b439fbd7dfcfd6f884d1dfb71de47845c4`.
GCC, platform Clang 19.0.1/LLD, Java 21.0.4 and Go 1.23.2 compile/run checks
pass; platform Ninja 1.9 runs. Source synchronization uses four workers and records progress
under `android-platform-build/sync-progress.json`. Its supervisor stops near
390 GB, leaving headroom below the approved 400 GB, or if host free space falls
below 50 GB.

The build uses `lineage_gsi_arm64-bp1a-userdebug`, with output under
`/build/out/octosense-platform`: first `aapt2`, then `TrebuchetQuickStep` after
the preflight succeeds. The supported `--soong-only` mode retains product
configuration and normal Soong dependency checks while building these Blueprint
modules. This generic product checks the platform dependency chain; it is not
the OnePlus ROM product. Each build is limited to two jobs in
a container with two CPUs and CPU affinity `0-1`. The original 24 GiB
memory cap was raised to 28 GiB within the existing Docker VM allocation. The supervisor measures the
combined source/output volume against the same disk limit. Per-container limits
do not change Docker Desktop's settings. Build scripts and results are retained
under `android-platform-build/` and `android-platform-build/upstream-build/`.
`continue-build.py` waits for the ROM-alignment container to exit successfully
and for its verified resolved manifest, then runs the two stages in order.
Before each build, all project revisions and working-tree edits are checked;
the only permitted source patch forwards the Go memory controls. A failed stage stops
the sequence; `pipeline-status.json` records its outcome. The upstream APK, if
built, is a baseline build artifact and is never automatically installed.

The initial bulk-checkout manifest SHA-256 is
`662f1e898f5a784cd278ec27f41061627b921732acc4b260ca4c4b20b79191e5`;
all 1,141 project revisions are immutable Git hashes. Sync completed in 3,255
seconds. The first generic-product attempt used `trunk_staging`, which selects
the Baklava preview configuration in this tree. It was stopped after this was
observed. `bp1a` inherits the release flags for Android 15, API 35 and `REL`.
The earlier attempt is retained separately; the corrected build must pass its
own preflight. Soong's nsjail self-test fails inside this Docker environment and
Soong disables its inner build sandbox. Docker's settings and privileges were
not changed to work around that limitation.

The plain `aosp_arm64` product subsequently failed during Soong analysis because
Lineage's global kernel generators lacked the Lineage board-variable exports.
The recipe now sources `build/envsetup.sh` and runs the included Lineage GSI
`lunch` target before building individual modules. This sets `LINEAGE_BUILD`
through Lineage's own environment setup. The product gate verifies the effective
`bp1a` aconfig set, Android 15/API 35/REL, the product and `LINEAGE_BUILD`.
`TARGET_RELEASE` itself is intentionally cleared by the Make configuration;
the earlier verification attempt incorrectly required it in a Make-variable
dump and is retained with the other failed attempts. The GSI product emits a
debug-policy configuration warning; this workflow builds tools/APKs only and
does not create or install a GSI image.

The first build with the verified Lineage configuration was killed during
Soong's full dependency analysis. Docker recorded a container OOM event at
05:54:24 UTC on 17 September (22:54:24 local time on 16 September). Its log,
recipe and event record are retained in `upstream-build-lineage-oom-24g/`.
No native APK was produced by that attempt.

The retry initially kept the 24 GiB container cap and Docker's existing 32 GiB
VM. It uses CPU affinity because Blueprint resets its worker count to the detected
CPU count. It also uses `GOGC=30` and `GOMEMLIMIT=12GiB`. Soong normally clears
the builder environment; a small, retained local patch forwards only those two
Go runtime settings. The patch and original/modified source hashes are recorded
separately from the resolved manifest. The manifest identifies upstream revisions;
it does not include this local build-tool patch. These settings trade additional
collection work for a smaller Go heap; the memory limit is soft and does not
cover all container memory. [Go GC guide](https://go.dev/doc/gc-guide#Memory_limit).
The supervisor now records cgroup memory usage, peak usage, OOM counters and CPU
limits. When analysis reached the cap, the build container alone was raised to
28 GiB after checking that other containers used about 1.4 GiB. The VM was not
restarted or resized. Soong completed dependency analysis with a container peak
of about 24.9 GiB and zero OOM events on this retry. The live legacy Make parser
was then stopped intentionally to switch to the recovered ROM revisions and
the supported module-only build mode. That run did not pass the full `aapt2`
preflight; its result and explicit stop reason remain in
`upstream-build-lineage-tuned-feasibility/` and
`upstream-build-before-rom-alignment/`.

The ROM-aligned Soong-only run completed all 977 `aapt2` build actions without
an OOM. Its post-build probe then failed because the recipe used the normal
Make output directory. In this mode, the Linux executable is at
`out/octosense-platform/soong/host/linux-x86/bin/aapt2`; running it reports
`Android Asset Packaging Tool (aapt) 2.19-octosense-adr0001`. The corrected
recipe also uses Soong's generated product path for the APK:
`out/octosense-platform/soong/target/product/generic_arm64/system/system_ext/priv-app/TrebuchetQuickStep/TrebuchetQuickStep.apk`.
The failed probe and original recipe are retained in
`upstream-build-soong-output-path-check/`. The compiler output retains a
nonfatal Windows COFF strip diagnostic. The complete corrected preflight passed
in 587 seconds, with a container memory peak of 26,773,372,928 bytes and no OOM
events. Its compiled executable hash and passing result are retained under
`upstream-build/`.
The memory-control patch helper now avoids rewriting an already-correct source
file, preserving its modification time for incremental builds.

The first upstream APK attempt then reached a dex-preoptimization dependency on
`product_packages.txt`, which the legacy Make image build generates. The failure
is retained in `upstream-build-soong-dexpreopt-dependency/`. The APK-only recipe
now uses the supported `WITH_DEXPREOPT=false` switch and verifies the resulting
`DisablePreopt`/`DisablePreoptBootImages` configuration. Java, DEX, R8 and resource
compilation remain enabled; ROM `.odex` files and boot images are not generated.
Android ART compilation and the resulting compiler state must be recorded on
the phone before performance comparisons. No missing-dependency allowance or
fabricated product-package list is used. The native APK retry reached 346 of
9,227 build actions before the user requested migration to a native x86 host.
It was stopped cleanly with SIGTERM, with zero OOM events; this is an intentional
stop, not a native APK build pass. The source volume and incremental outputs
remain available. Migration identity, SSH rejection and stop evidence are in
`native-host-migration.json` and `upstream-build/stopped-reason.json`.
`--quickstep-only` requires the existing passing preflight and matching source
manifest. New build containers use a stable hostname for incremental builds.

The phone runs `22.2-20260708-NIGHTLY-enchilada`, incremental `21be58cea4`.
Its build manifest has now been read from `/product/etc/build-manifest.xml`
without changing the phone. SHA-256 is
`eaecfdae859bde5555cfd1589e63ecb8b26b5be96ebdb921122c81c738f23014`.
It pins 1,145 projects. Relative to the initial checkout, 1,072 revisions matched,
69 differed and four OnePlus device/kernel projects were absent. The installed Trebuchet revision is
`dfc9b2347e903ad6930274dafcfdba3e594074ab`; the installed framework revision is
`ff7620a38e54c5f7ec14a5b8ccc5be1ba41e2b1b`. The inspected observer, fallback
controller, BuildConfig, Blueprint and common/Quickstep manifests are unchanged
between that Trebuchet revision and the earlier reference. Alignment completed
successfully in 287 seconds. The resulting manifest SHA-256 is
`9d08129ad7be4d2ef24b5596790023f4996aeb37170298a9c2882c21f236ef66`;
all 1,145 project identities and revisions match the installed manifest.

A local manifest pins every selected project. Four repositories that switched
from AOSP to Lineage forks after this ROM were replaced with their original
AOSP identities. Their working trees were verified clean first; force-sync was
limited to those four Git-directory associations, with no force-checkout.
`repo` reported hook-directory differences during replacement and completed
successfully. The final revision comparison passes. The build's source gate
was satisfied only after checking all project revisions and the recorded local
patch. Evidence is retained under `quickstep-source-inspection/rom-manifest-lookup/`
and `platform-setup/rom-source-alignment/` in the ADR artifact directory.

The current official device API lists September builds, so it does not provide
this July build in its returned list. The phone's installed manifest supplies
the specific source revisions; it does not by itself reproduce signing keys,
excluded proprietary sources or device build configuration. This setup approval
permits building candidate APKs; native controller installation and ROM Recents
reconfiguration remain a separate deployment decision after artifacts and
rollback are ready.

## Native integration checks after checkout

1. Build upstream Trebuchet Quickstep first, using the platform dependencies and
   generated sources. Retain the resolved manifest and selected product/config.
2. Add an OctoSense app module with the existing Quickstep package identity,
   contracts and Home layout service. Use Soong's supported `package_name`
   property and inspect the merged manifest/provider authorities. The Gradle
   layout-service prototype remains available as rollback.
3. Adapt `OverviewComponentObserver`: upstream dereferences its own HOME intent's
   resolved activity and assumes that an integrated launcher exists. A separate
   Recents package must handle that missing activity and use fallback overview.
   Verify null/default-Home changes and boot/locked states before deployment.
4. Feed authenticated, fresh Home geometry into `FallbackSwipeHandler` without
   per-frame Binder work. A target must match component, user, display, rotation
   and the current transition. Invalid, stale or missing geometry must retain
   the native generic Home animation. Preserve native cancel/finish cleanup.
5. Inspect the resulting permissions, package identity, signer, hidden-API access,
   controller service and native Recents entry points. Prepare the deployment
   module and restoration artifacts before requesting controller deployment.

The inspected `build/make/core/build_id.mk` is `BP1A.250505.005`, matching the
phone's `ro.system.build.id`. The later manifest comparison establishes the
Lineage project revisions. The generic product does not establish the OnePlus
product configuration.
The earlier Trebuchet reference is commit
`2bee8237d1bb4bc79c348cc7178a780e44a822a3`; use the checkout's actual resolved
revision for implementation and record any difference.

## Separate Recents module preparation

The stager is pinned to the installed ROM's Trebuchet revision
`dfc9b2347e903ad6930274dafcfdba3e594074ab`. Its six inspected integration/build
files are byte-identical to the earlier reference.
[stage-quickstep.py](../../android/platform-build/stage-quickstep.py) prepares an
`OctoSenseQuickstep` Soong module with the existing contracts and authenticated
Home layout service. Its read-only `--check` mode passes against that revision;
the Blueprint fragment parses and the XML files are well-formed. This is source
preparation evidence, not an APK build or phone validation result.

The module uses its own native library and `BuildConfig.APPLICATION_ID`. Changing
the final APK package alone would leave compiled provider identifiers pointing
at `com.android.launcher3`. The source manifest also uses the OctoSense package,
so `${applicationId}` authorities are resolved correctly during manifest merge.
The merged APK still needs inspection before signing or deployment.

The prepared observer copy permits the absence of an integrated HOME activity.
It selects fallback Recents even while the default Home is temporarily null and
guards null components during configuration checks. The original upstream Java
files remain intact. HOME/secondary-HOME, launcher pin confirmation and the
launcher notification listener are excluded from this separate APK; their
responsibilities remain in Makepad Home and System Bridge.

Writing the staged sources requires a passing upstream Quickstep baseline,
verified ROM source provenance, the matching retained APK hash and the shared
build lock. A negative check confirms that the failed feasibility result is
rejected before staging. The script refuses unexpected source revisions
or edits to its previously staged files. The platform build uses a test key;
the existing OctoSense key stays on the Mac and must sign a reviewed candidate
before any separately approved installation. The stager now prepares a separate
`FallbackSwipeHandler` copy that consumes `HomeTransitions` targets and retains
native spring/surface ownership. It handles native cancellation, early handler
replacement and Recents completion even after Home task appearance. Only the
Makepad Home and a single-app return qualify; PiP and split cases keep their
native paths. The original fallback source is also checked for unexpected edits.
The 17-file preparation check and Blueprint/XML parsing pass. Native controller
compilation, ROM grants/binding and actual gesture behavior remain pending.

The shared coordinator and Home callback path implement positive transition IDs,
session/epoch ordering, fresh renderer publications and exact component/profile,
display and rotation matching. Profile/package queries occur on the service
worker. Frames read an immutable target and write a progress scalar; a separate
timer coalesces changed progress at most 20 times per second and stops on terminal
events or the five-second transition deadline. A target expires after one second
without fresh publication. Invalidations, cancellation and supersession clear
targets immediately. The SDK prototype keeps controller capability false; only
the platform manifest enables the native endpoint, with phone-validation status
still false. Native permission requests do not prove grants.

The final coordinator revision passes 74 phone checks; the Home schema and
updated layout transport pass 42 and 75 respectively. The Home geometry/widget
regression also passes. These tests cover local coordination and the layout
prototype; they do not exercise the staged native gesture controller. The retained
normal Home and original layout-service APKs were restored after testing. See
the implementation record for exact hashes and remaining gates.

[inspect-quickstep-apk.py](../../android/platform-build/inspect-quickstep-apk.py)
checks the signed candidate's actual binary manifest and certificate: package,
API 35 compatibility, signature-protected Home service, native controller/Recents
entries, provider authorities, and absence of competing HOME/pin/listener
registrations. It correctly rejects the existing layout-only prototype for
missing native controller components. This check does not establish actual ROM
grants, SystemUI binding, transitions or performance.
