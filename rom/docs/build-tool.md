# The build tool is part of the framework

`cargo-makepad` is not a generic packager. Its Android half carries the
Java activity (`tools/cargo_makepad/src/android/java/dev/makepad/android/
MakepadActivity.java`), and that activity *is* the Android side of the
framework: every JNI call the Rust backend makes lands in a method the
activity declares. The tool compiles that Java from the checkout it was
**built** in — its own `CARGO_MANIFEST_DIR` is baked into the binary — while
the Rust comes from `../makepad` through the `[patch]` section of
`Cargo.toml`, whatever tool runs the build. So the tool and the framework
must come from the same fork revision, and the installed binary is the one
thing the pin cannot see.

## How the fork's tool differs from upstream

Measured on 2026-09-18, fork `OctoSense-org/makepad` `main` at `bb45d411`
against upstream `makepad/makepad` at `16ccd92`:

| `tools/cargo_makepad/src/…` | Upstream | Fork | Differing lines |
|---|---|---|---|
| `android/java/…/MakepadActivity.java` | 3104 lines | 4395 | 1301 |
| `android/java/…/MakepadNative.java` | 91 | 134 | 43 |
| `android/compile.rs` | 3281 | 3324 | 45 |
| `font_assets.rs` | 807 | 852 | 45 |
| `apple/compile.rs` | 1383 | 1383 | 0 |

`android/mod.rs`, `MakepadNetwork.java` and `open_harmony/compile.rs` differ
too. Almost all of it is the activity. What it has that upstream's does not:

- **The system browser** — `spawnSystemBrowser(long, String, boolean)` and
  its WebView overlay, behind the News reader and the web app cards.
- **The HOME intent forward** — `onHomeIntent`, which is what lets OctoSense
  be the phone's Home app.
- **Deep links and share intents** — `onDeepLink`.
- **One activity instance per process** — `sNativeActivity`: a launcher
  started by a plain component intent gets a second instance from the Home
  button, and the older one must stop reaching native (MOBILE-05).
- In `font_assets.rs`: this crate builds `src/main.rs` as a lib and a bin,
  so the binary carries two identical font manifests; the fork's packager
  accepts that, upstream's refuses it.

Upstream's tool will therefore never build this app correctly; that is
expected, not a bug. If these features are to reach upstream one day,
`MakepadActivity.java` is the file, and the diff is about 1300 lines.

## What goes wrong with the wrong tool

A stock tool with the fork's Rust builds and installs without complaint —
the Rust compiles against `../makepad` either way — and fails at the first
JNI call the stock activity lacks. With News that is the first tap on a
headline:

```
Android panic hook: … payload=JNI method not found:
spawnSystemBrowser(JLjava/lang/String;Z)V — the makepad buildtool is out
of sync with the framework.
```

The render thread is gone after that panic; the shell keeps its last frame
and stops responding, and the log shows `surfaceOnSurfaceDestroyed: render
thread did not acknowledge within 2s`. Force-stop the app and rebuild with
the right tool.

## Keeping `cargo makepad` on the fork's tool

`cargo makepad` runs `~/.cargo/bin/cargo-makepad`. Install the fork's tool
there once, and again whenever the fork's tool code or Java changes (a pin
move that touches `tools/cargo_makepad` is the usual trigger):

```sh
cargo install --path ../makepad/tools/cargo_makepad --force
```

To see which checkout an installed tool reads its Java from:

```sh
strings ~/.cargo/bin/cargo-makepad | grep -o '/Users/[^ "]*tools/cargo_makepad' | sort -u
```

The Android toolchain lives under the tool's own directory
(`tools/cargo_makepad/android_33_macos_aarch64`); `cargo makepad android
install-toolchain` puts it there, or a symlink to another tool's copy does.

## The gap

Nothing checks that the installed binary was built from the pinned fork:
`tools/setup-native.py --check` verifies the sibling checkouts and the
Cargo graph, not `~/.cargo/bin`. A check in the tool (compare its baked
checkout against the fork revision the manifests pin) or in
`setup-native.py` would close it.
