# MakeOS

A Makepad desktop that hosts compatible applications inside one window. The shell comes from Makepad's WM app; framework libraries remain external Cargo dependencies pinned to the same upstream commit.

## Run

Install stable Rust and the native development tools for your OS. On macOS, install Xcode Command Line Tools (`xcode-select --install`) if needed. Validated with Rust 1.98.1 on macOS.

From this directory:

```sh
cargo run
```

Open **Apps → Reference** from the top-left menu. The reference app has a counter and text input, and runs as a separate process inside a tile. Its first launch builds the release executable; build progress appears in the tile. Later launches reuse Cargo's build cache. To build both executables ahead of time:

```sh
cargo build --release --workspace
cargo run --release
```

The first build downloads Makepad and other dependencies. No sibling Makepad checkout, Studio process, model download, or wallpaper download is required. Fonts and other framework resources are read from Cargo's dependency checkout during source development, so keep that cache available.

The default desktop starts empty. AI assistant startup, background app prewarming, demo filesystem generation, and wallpaper downloads are off. **System → Quit MakeOS** closes the desktop and its hosted processes.

Use **⌘Space** for the menu, **⌘W** to close a tile, **⌘F** for tile fullscreen, **⌘1…0** to switch workspaces, and **⌘Shift1…0** to move the focused tile. The menu's **Learn → Keybindings** lists the inherited bindings; shortcuts for apps absent from your catalog report that the app is unavailable.

## Add an app

Applications must be compatible Makepad applications that support the `--stdin-loop` hosting protocol. Use the same Makepad revision as this project; the protocol is not a stable compatibility boundary across arbitrary revisions. Start from [apps/reference](apps/reference).

The default catalog is [config/apps.json](config/apps.json). To keep a personal catalog, create `~/.makeos/apps.json`; it replaces the default catalog. An explicit catalog can be selected with:

```sh
cargo run -- --apps /path/to/apps.json
```

A catalog is a JSON array. Each entry chooses either a Cargo manifest or an executable:

```json
[
  {
    "id": "notes",
    "label": "Notes",
    "manifest": "../notes/Cargo.toml",
    "package": "my-notes",
    "bin": "notes",
    "policy": "new",
    "args": []
  },
  {
    "id": "installed-notes",
    "label": "Installed Notes",
    "executable": "/opt/my-apps/notes",
    "policy": "focus"
  }
]
```

Relative paths resolve from the catalog's directory. Arguments are passed literally, without a shell. `policy: "new"` opens a new instance; `"focus"` focuses an existing instance and is the default. Restart MakeOS after editing the catalog. Only apps with available launch targets appear in the launcher. Missing manifests, invalid catalogs, and failed starts are reported in the desktop/logs.

MakeOS adds the hosting arguments and connection settings itself. Do not add `--stdin-loop` or Studio connection variables to the catalog. For executable registrations, supply the app's resources as required by that app's packaging.

## Settings and optional features

MakeOS state lives under `~/.makeos`; `MAKEOS_HOME` selects another directory. Theme files and hosting overrides retain the upstream `wm/` layout inside that directory. Hosted apps inherit the MakeOS state root through Makepad's compatible `MAKEPAD_HOME` setting.

Optional launch flags are `--assistant`, `--prewarm`, `--demo-home`, and `--download-wallpapers`. Assistant/prewarm flags require matching apps in your catalog. Theme importing remains an explicit action in the theme menu.

Upstream's linked-module infrastructure is retained behind `app-sheets`, `app-photos`, and `app-aichat` Cargo features; all are off by default. A module must be linked and selected with `--module <id>` or `wm/apps.splash`. This initial milestone validates process hosting. It does not provide runtime loading of native shared libraries or embedding of unrelated native desktop windows.

## Upstream updates

[upstream/makepad.json](upstream/makepad.json) records every imported file, its original path/hash, and the matching framework revision. The source baseline is `83a00d2801e4864c42c3a40e85186a8b1743fd84`.

Run this daily, or after any upstream pull. With Python 3.11+, update the Makepad
checkout using your normal Git workflow, then run one command from MakeOS:

```sh
git -C ../makepad pull --ff-only
python3 scripts/upstream.py sync
```

`sync` defaults to the sibling checkout's current `HEAD`. When that matches the
recorded revision, it exits without building. Otherwise it requires a clean
MakeOS tree, saves comparison diffs, stages the merge, updates all dependency
pins and the lockfile, runs compile/Rust/Python checks, builds both profiles,
and runs both native smoke modes. It reuses an ignored staging build cache.

After every check passes, it creates a unique `sync/makepad-<revision>` branch
and applies the verified changes, leaving them unstaged and uncommitted. You
then review the diff and report, commit, and merge. Conflicts or failed checks
stop with diagnostics and preserve the live import. The command never pulls,
commits, merges branches, or pushes. Full sync currently requires macOS GUI
access; a headless session cannot pass its native runtime checks.

Reports and captured frames are under `target/makepad-sync/reports/`; the command
prints the exact directory. Use `--source /path/to/makepad` or `--to <commit>`
when needed. See the [full workflow and conflict recovery](docs/upstream.md) for
manual commands and how to investigate a failed candidate.

## Verification and scope

```sh
cargo test --locked --workspace
python3 -m unittest discover -s scripts -p 'test_*.py'
```

The native smoke test opens and closes its own test windows, isolates settings in a temporary directory, and records app-provided frames/logs. After dependencies are downloaded:

```sh
cargo build --release --locked --workspace
python3 scripts/smoke.py
python3 scripts/smoke.py --cargo-run --default-catalog
```

The first smoke command checks hosted input, workspace movement, fullscreen resizing, independent instances, failed launches, and quitting during an unfinished build. The second uses exactly `cargo run` with the shipped catalog. Python supplies app-local remote control and isolated state through the environment; neither is required for normal use. Smoke runs set Cargo offline and require GUI access.

See the [validation record](docs/validation.md). Source builds and process hosting are the initial target on macOS. Linux/Windows branches are retained but have not been validated here. A relocatable `.app`, installer, web/mobile delivery, and a Linux session compositor are separate work.

The copied Makepad source is covered by its [original MIT notice](LICENSES/Makepad-MIT.txt). Dependencies retain their respective licenses.
