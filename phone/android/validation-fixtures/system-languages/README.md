# System language validation fixture

This independently signed public-SDK fixture observes global locales through
`LocaleManager`, drives the built-in UI through Android accessibility, and renders
actual English/French/Simplified Chinese/Traditional Chinese/Arabic resources.
It has no locale setter and never calls an OctoSense Binder service. Its optional
shell `ConfigurationObservation` entry point only reads authoritative
ActivityManager configuration, including `userSetLocale`.

`home/android/scripts/run-system-languages-probe.py` accepts `--scenario
unavailable` only on emulator5556 and `--scenario native` only on the disposable
emulator5560/OctoSense_ROM_Roles_API35 AVD. The fixture must not already be
installed. `--build-only` compiles the APK without accessing a device. Each run
requires a `--receipt` path and the usual `--adb`, `--sdk`, and `--java-home` paths.
The child owns only5556; the parent runs the native scenario on5560.

The ordinary scenario proves disabled mutations and exact trusted native
recovery/Back. The native scenario retains every baseline language while adding
and reordering fixture selections. It verifies actual global order, UI review and
Cancel, real resource language/RTL and Activity recreation, Home restart, and
stale review after a finite regional-preference change. Native filtering is used
at every language/region/numbering level. All language writes and restoration
use the built-in reviewed UI. There is no mutation retry.

The finite regional preference fixture uses only `und-u-hc-h23` and
`und-u-hc-h12`; its exact original null/empty/value is restored. The outer runner
restores raw locale provider rows only after the original authoritative ordered
configuration has already been restored. It reports Configuration metadata
separately: native LocalePicker sets `userSetLocale=true`, and the fixture does
not forge a hidden setter to reset it. A locale cleanup failure remains a failed
receipt and does not trigger a guessed locale write.

Numbering choices are tested only if the actual catalog offers them. The receipt
must distinguish implementation/unit coverage from such native device coverage.
No physical device is allowed.

Final Home2539/Broker r2 native acceptance passed 248 checks on the disposable
5560 image, including seven Apply operations preserving Home's PID and Java
Activity identity, actual French/Chinese/Arabic text and Arabic RTL, and exact
original ordered Configuration/provider rows/userSetLocale restoration. The tested selections did not traverse a numbering-system leaf; native device
coverage for that branch remains open. The audit retains the earlier nonpublic-collector and Home lifecycle
product failures, and the Unicode-tag/retired-scroll fixture corrections.
