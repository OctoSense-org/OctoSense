# Photos Memories Implementation Plan

**Status (2026-10-04):** Done: merged in [#266](https://github.com/OctoSense-org/OctoSense/pull/266) (a3098486), including f6ba06a6 (memory snapshots use the account data path) and a5f1a6e1 (Photos' agent stays read-only). Live providers and physical phones remain unverified.

**Goal:** Turn Photos' preset moment card into a browsable collection of saved AI-curated stories, optionally guided by a prompt.

**Architecture:** Photos calls the existing host-owned `model.complete` service with bounded catalog metadata and a strict story schema. It validates membership and uniqueness of photo IDs, persists accepted stories separately from the existing album store, and keeps local moments available without an AI provider. No image bytes or credentials enter the request.

**Tech stack:** Existing Splash bundle, Makepad UI and script VM, OctoSense's model host service.

## Design

Visual thesis: image-led stories on Photos' calm white surface, using its existing type and blue accent.
Content: Collections keeps a featured memory and links to the full Memories browser. The browser offers an optional prompt and a Create memories action, saved AI stories and local moments. A detail view shows the title, short narrative, photo grid and Play action.
Interaction: preserve swipe navigation; provide explicit pause/resume, previous/next controls; cancel pending generation without accepting late results.

The initial release curates the bundled sample library's metadata. It does not claim to analyze image pixels or import a device library. Generation is user initiated. Loading, missing provider, budget/rate refusal, invalid output, timeout, empty result and storage failure must leave previously saved stories usable. Existing albums and favorites retain their v1 format.

## Implementation and verification

1. Add executable Splash behavior tests covering valid stories, invented/duplicate IDs, malformed output, persistence, existing albums, no provider, cancellation and stale replies. Observe missing-feature failures before implementation.
2. In `apps/photos/bundle/main.splash`, add bounded metadata requests, a supported JSON schema, strict acceptance checks and separate versioned memory storage. Add `model` to `manifest.json`.
3. Add Memories navigation, prompt/loading/error UI, detail and slideshow controls, matching the existing components. Use local photo IDs only for rendering.
4. Run regression tests with the pinned VM, run hidden host/shell UI checks, and inspect captures and logs. Add the repeatable check to existing CI as appropriate.
5. Update `apps/README.md` and `apps/README.zh-CN.md` together to explain metadata curation, host service needs and verified limits. Review the complete diff.

## Completed verification (2026-10-01)

- `cargo test --locked -p octosense-llm-service`: 84 passed, one pre-existing ignored test. Includes 11 Photos tests in the actual pinned Splash VM and validation against the model host's schema compiler.
- `cargo clippy --locked -p octosense-llm-service --test photos_memories --no-deps -- -D warnings`: passed.
- `cargo build --locked -p octosense --no-default-features --features app-hub`: passed.
- `python3 apps/photos/tests/ui.py`: passed. Two hidden shell launches with isolated temporary data and a local mock provider cover missing-provider guidance, typed prompts, the actual host completion path, navigation while generation runs, persistence, story detail, slideshow controls and reopening. Captures are written to ignored `target/photos-memories-ui/` and were visually inspected.
- `python3 tools/setup.py --check --cargo`, `python3 tools/native_apps.py --check`, `git diff --check`: passed.

The initial regression run failed on the six absent AI behaviors, and the slideshow test failed before its implementation. Code review identified partial-write recovery as a gap; its regression reproduced loss of history before the fix and passes with alternating, revisioned snapshots. Each write preserves the previous verified snapshot, and loading selects the newest valid one. A second review found no remaining concrete issues.

Live AI providers and physical phones remain **unverified**. This feature uses the existing sample catalog's metadata, not pixel analysis or device-library import. The hidden-shell test uses the desktop's thumbnail fallback; full-size photo assets remain an optional shell mount. No production profile or personal app data was used for validation.

## Updated ecosystem compatibility review (2026-10-01)

Refreshed all six related repositories' `origin/main` references. Fast-forwarded this feature branch from `461acab` to OctoSense `07516e5`; the incoming News fixes do not overlap the Photos changes. Reviewed App Hub `58c3c8a`, Design Flow `e0851725`, OctoScript `30a21b7`, Octoscript-Makepad `e0df0b5` and Makepad `8e1bbf1f8`. Builds retain OctoSense's declared runtime pins and patch stack, rather than substituting sibling repositories' floating main revisions.

- Added a Photos package-admission regression using the published `octosense-app-contract` 1.x workspace dependency: stamp the system bundle digest, parse, admit, then resolve its policy with `HostLimits::system()`. It verifies `model` and `storage` grants, host-owned provider networking and device-local storage. The schema-1 manifest and existing bundle structure remain compatible; no app agent is needed for `model.complete`.
- App Hub bounds pending host requests with a default 60-second deadline. Photos now gives explicit timeout guidance and permits retry; its 180-second local timer remains a fallback for absent callbacks. The new regression verifies saved stories survive, late replies are ignored and another request can start. It failed on the generic error message before the change.
- Updated both app READMEs to name the new contract source, remove a stale App Hub revision and explain the timeout behavior. Some Design Flow AI-service status text still describes model calls as forthcoming; the running OctoSense implementation and its service tests are the authority for this feature.

Repeated validation on `07516e5`: 86 model-service tests passed (13 Photos tests; one existing keychain test ignored), Clippy passed, the current shell built, the hidden-shell smoke test passed, and setup, native-app generation and diff checks passed. Live providers and physical devices remain unverified.

## Draft PR preparation (2026-10-01)

Fast-forwarded the feature branch to `be0700a` before publication. The incoming model-service change extends its request deadline to 270 seconds to cover both provider attempts. Photos now waits 300 seconds before its own missing-callback fallback, so it does not discard a response while those attempts are still allowed to run. A new regression failed with the old 180-second fallback and passes with the updated deadline. Both user-facing READMEs describe the current timing.

Final pre-publication validation: 89 model-service tests passed (14 Photos tests; one existing keychain test ignored), Clippy passed, the shell built with the current runtime patch stack, and the hidden-shell smoke test passed. Setup, native-app generation, Python compilation and diff checks also passed. Live-provider and physical-device coverage remains unverified.
