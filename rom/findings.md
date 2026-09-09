# MakeOS findings

## Initial observations
- `/Users/guofoo/git/mp/makeos` is empty; no Cargo package, Git metadata, or local instructions were present.
- Source app: `/Users/guofoo/git/mp/makepad/apps/wm`.
- User requests discussion and planning before implementation.
- Makepad's source runbook documents built-in `--remote` controls for later runtime verification and asks that agent-created test instances be closed afterward.

## Evidence still to collect
- Source commit, working-tree modifications, package manifest, file inventory.
- Transitive Cargo dependency closure and runtime resource paths.
- Actual embedded-app loading model and platform limitations.
- Minimal import inventory and repeatable upstream comparison workflow.

## Source snapshot and initial dependency boundary
- Source HEAD: `83a00d2801e4864c42c3a40e85186a8b1743fd84`, branch `work`, origin `git@github.com:makepad/makepad.git`.
- No tracked source changes reported. Four unrelated example directories are untracked.
- WM is a binary-only package (`makepad-wm`, binary `wm`); its manifest has no library entry point.
- Eight non-optional direct Makepad dependencies: widgets, studio-protocol, network, wm-api, ai-services, strict-json, app-module, wm-theme.
- Three optional application crates are enabled by default: sheets, photos, aichat (all with their own default features disabled).
- WM contains both child-process hosting and statically linked module hosting. Source manifest labels Linux session/compositor mode as future work.
- `shell/ui.rs` has font references reaching outside the package (`self:../../widgets/resources/...`), while icons live under WM resources.
- The Makepad root includes Cargo patches for bitflags, smallvec, windows-link; a separate root must assess whether these need reproducing.
- Installed toolchain: rustc 1.98.1; Cargo 1.98.1.

## Extraction hazards confirmed in source
- `clients.rs:303` detects a Makepad checkout via an environment override or ancestor `Cargo.toml` plus `local/`; it otherwise resolves binaries beside WM. This needs a MakeOS-specific app location policy.
- The process registry is a hard-coded curated list. Supporting user-added applications independently requires a small external manifest/registration seam; module overrides alone do not add apps.
- Process launch uses `cargo run --release -p ...` in a Makepad checkout, passing `--stdin-loop` and `STUDIO_HOST`, `STUDIO_BUILD`, `STUDIO_CRATE`. It has existing child output and process-group cleanup handling worth preserving.
- `main.rs:handle_startup` auto-starts aichat (even with a closed pane), starts warm-pool management, and fetches missing wallpapers. Warm capacity covers two terminals and one each of browser/files/task.
- Theme defaults are embedded Rust strings. Theme/user state currently resides under `MAKEPAD_HOME` or `~/.makepad/wm`; MakeOS should have its own default state directory while preserving compatible child protocol environment names where needed.
- `config/omarchy/shell.json` appears in explanatory comments, but no such source directory exists; it is not yet evidence of a required file to copy.
- Exact remote availability of the source commit is not verified: web opening the GitHub commit returned an internal error.
- Cargo documentation confirms Git dependencies can select crates inside a repository by name and pin `rev`; Cargo still fetches the Git repository. This minimizes files maintained in MakeOS, not necessarily first-download size.

## Reference documentation
- https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html
- https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html

## Inventory and reuse conclusions
- Tracked WM footprint: 69 files, 881,317 bytes: 27 Rust files (23,057 lines, including tests), 41 SVG icons, and one manifest. The import can remain under 1 MB before any optional added assets or fixture code.
- Read-only `cargo metadata --offline --locked --no-deps --format-version 1` succeeded. It confirms WM has only a binary target and the dependency/feature inventory above. This is manifest validation, not a build or transitive dependency resolution test.
- `origin/work` locally contains the chosen source commit. The live remote could not be verified through the web tool (commit and raw manifest fetches failed); implementation must verify actual Cargo fetchability.
- `makepad-widgets` currently declares version 2.0.0, but that does not establish that all WM APIs are available in published registry releases. A single shared Git revision is the safer initial dependency choice.
- `makepad-ai-services` has an empty default feature set; retaining its wire/service code does not inherently enable the model engine. Removing it would require editing the WM's existing bus and module interfaces.
- Terminal has a reusable library for PTY/session/terminal/widget functionality, but its complete application remains in `src/main.rs` plus `src/ai.rs`. It cannot be turned into a complete hosted executable with a one-line call to a library app entry point.
- Sheets exports a real `SHEETS_MODULE`; this offers an already implemented lightweight module candidate without copying Sheets code. Its standalone executable still has its own application main.
- Cargo library dependencies do not supply ready-built sibling application executables. A process-hosting milestone needs a bundled executable target, an explicitly managed build/install step, or a development manifest launch.
- Makepad's resource resolver supports named crate references and development-time files in dependency checkout paths. Fix copied WM font references to identify the widgets crate; packaged distribution requires a separate resource staging check.
- WM launcher currently treats a linked module as available even when default hosting is Process. If a module is bundled, availability and actual selected launch mode must agree; otherwise a visible row can still lead to a missing process binary.
- Upstream source and Cargo.lock remained unchanged by inspection.

## Daily sync automation findings
- The existing update verifier runs Cargo metadata/check/tests and Python fixtures, but no release build or GUI smoke tests.
- A fixed ignored candidate directory can preserve its own `target/` cache and keep runtime resource/catalog discovery rooted in the candidate. A shared target outside it could make the app discover the live project instead.
- The source checkout remains at the current baseline on 2026-09-06; use local Git fixtures to verify actual revision transitions.

## Fork feature takeover findings — 2026-09-08
- The MakeOS desktop style depends on seven changed widgets paths outside apps/wm. Pinning all crates to published guofoo/makepad beb3857a provides these without vendoring framework code.
- The provenance baseline now uses the fork, with default_source ../guofoo-makepad. This checkout tracks official upstream/work locally, so the documented fork pull names origin work explicitly. Incorporating official updates into the fork remains the user's source Git workflow.
- Safe cached-view snapshots and wallpaper redraw are now provided by widgets; the temporary standalone WM implementations can be removed.
- Persistent widget-tree child enumeration is necessary for dynamically hosted apps: one-time insertion alone loses surviving entries when a sibling closes and the tree refreshes. A floating desk also needs its turtle area, not its tiling border's stale area.
- Successful screenshots alone do not detect skipped shaders; native smoke now rejects runtime shader/error logs as well as checking app input and state.

## Omarchy wallpaper — 2026-09-08
- MakeOS bundles Tokyo Night colors but no wallpaper and deliberately gates downloads behind --download-wallpapers. Its separate state directory has no Tokyo Night backgrounds. The old Makepad state has the original 0-winding-road.webp (653,482 bytes), first in the sorted wallpaper list.
- Keep installed-background discovery separate from the embedded fallback so explicit downloads still work. Style switching currently tests only installed files, so it must use the actual load result to expose the fallback.
- Async Image visibility precedes decoding. Native startup validation now waits for the rendered wallpaper frame; the default gradient is only a few KB, while the fixed photographic frame exceeds 50 KB. The SVG Image path does not expose a raster area in snapshots, so Omarchy visibility assertions are scoped to its raster wallpaper.

## Shared Qwen configuration — 2026-09-08
- The fork AI engine searches MAKEPAD_AI_CHAT_MODEL, then MAKEPAD_HOME/weights recursively, then checkout-local models. MakeOS sets child MAKEPAD_HOME to its separate state home. A symlink to the existing GGUF solves discovery without changing the app or sharing the full state home.
- The pinned aichat settings implementation still uses ~/.makepad/aichat/settings directly, independent of MAKEPAD_HOME. No settings file exists here, so defaults select Local and local-only; no settings were modified.
- Model inference through the hosted child passed; two automated host-pane input attempts failed to submit. Retain this separate input-routing observation for subsequent UI work.

- Contributor setup source verified on 2026-09-09: unsloth/Qwen3.5-9B-GGUF revision 24fadbaba5891f3965d66ea0e2e4aa259cd38c77 publishes the tested file with SHA-256 6f5d30666c2d8ae16a306e616d95341dcf3cc46810df84d7e6f5a7d1e4c1b293; hashing the existing local GGUF produced the same digest. Source: https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/blob/24fadbaba5891f3965d66ea0e2e4aa259cd38c77/Qwen3.5-9B-UD-Q4_K_XL.gguf
