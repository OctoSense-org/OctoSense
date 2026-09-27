# Sync OctoSense-mobile into Home Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Bring every change made in `OctoSense-org/OctoSense-mobile` after the Home import into `home/`, so the mobile repository can be archived and all further Home work happens here.

**Architecture:** Mobile history is already part of this repository. Home was imported by an unsquashed subtree merge (`69b68b8`, 2026-09-21) of mobile `653cd67`. One more subtree merge of mobile `main` (`git merge -X subtree=home`) carries the nine later commits with their authorship and history, and Git computes the three-way merge from the real common base. Nine files conflict. Their resolutions below were applied and validated in a scratch worktree on 2026-09-25. One commit from open mobile PR #10 is then cherry-picked the same way. Calendar (open PR #11) is a separate follow-up.

**Tech Stack:** Git subtree merges, Rust/Cargo workspace in `home/`, Makepad, Python product scripts, Android APK build via `scripts/build-home.sh`.

---

## Findings

### Where the repositories diverge

| | Commit | Date |
| --- | --- | --- |
| Last mobile commit imported into `home/` | `653cd67` Merge PR #43 `fix/home-hint-placement` | 2026-09-21 |
| Import commit in this repository | `69b68b8` Import OctoSense Home and its history into the ROM product | 2026-09-21 |
| Mobile `main` now | `65488bb` Merge PR #50 `chore/runtime-c4c96822` | 2026-09-24 |
| This repository's `main` now | `cfbf4e4` Merge PR #11 `chore/runtime-c4c96822` | 2026-09-24 |

### Mobile `main` since the import (9 commits, PRs #44–#50)

| Mobile commit | PR | Content | State in `home/` |
| --- | --- | --- | --- |
| `c3a366a` | #44 | Swipe-start chevron cue above system navigation | Missing. Conflicts with this repository's floating navigation (`199b42f`) in `mobile_surface.rs`. |
| `e4fa5cd` | #45 | Generated family portraits and prompts (`scripts/individuals/`, 15 MB PNG) | Missing; clean. |
| `6824f92` | #45 | Native App Hub `apps/app-hub` (`octosense-app-hub-app`): Today/Apps/Search/Library, verified installs and updates, `hub:<id>` identities, shared icons, 108 design/evidence files | Missing. `home/` instead wires `octosense-appstore` directly, ported from mobile PR #39. Conflicts in `Cargo.toml`, `apps.rs`, `main.rs`, `setup-native.py`. |
| `ad485ec` | #46 | Untrack App Store reference screenshots | Missing; merges clean. |
| `4e5a495` | #47 | Maps directions on Android 9 via rustls TLS 1.3 | Missing. The phone this repo tests on (OnePlus 6T) runs Android 9. Conflicts only in `apps/maps/Cargo.toml`. |
| `22d5489`, `deceb2f` | #48 | Runtime lock 36ad1f22, drop makepad override | Already mirrored as this repository's `f1a212e`. |
| `cdd9811` | #49 | AppCard main 7f468bb9 | Already mirrored as `6ed7283`. |
| `144aaf3` | #50 | Runtime c4c96822 / makepad 1d3d383e / AppCard 9e8e4898 | Already mirrored as `3976685`. `native-runtime.lock.json` and four app manifests are byte-identical. |

### Work outside mobile `main`

| Item | Disposition |
| --- | --- |
| Open PR #10 `ohos/mate70-air` (4 commits) | `ca9f563` (native_mobile cfg) and `0e41bf1` (perf traces) are already in `home/`. `8326864` (AppCard transport pin) is superseded by the AppCard 9e8e4898 pin. **`78e192c` (tick reason in `mobile_app.rs`) is missing and applies cleanly → Task 10.** |
| Open PR #11 `feat/calendar-module` (8 commits) | Mail, Camera, OHOS and store wiring already landed by other routes. **Calendar module hosting is the only missing piece.** Its source exists in OctoScript-App-Design-Flow (formerly Octoscript-AppCard), `apps/calendar/native` at `cbbda4da`. → Separate follow-up (Task 14). |
| Unreferenced mobile commit `45dbbfb` (“Host standalone Mail”, named in `docs/home-migration.md`) | Superseded: `allowBackup="false"`, `phone_client_texture` and Mail hosting are all in `home/`. No action. |
| Branches `docs/adr-*`, `feat/appstore-module` | Fully contained in mobile `main` or in PR #11. |
| Uncommitted notes in the mobile checkout (`findings.md`, `progress.md`, `task_plan.md`, +32 lines) | Session notes. Ask the owner whether to keep them before archiving. |

## Decisions

1. **App Hub: adopt mobile's native App Hub.** *(Recommended default. The owner did not answer this question; confirm before Task 4.)* `apps/app-hub` wraps the same backend revision `97c2a1f` and re-exports `octosense_appstore::{data_root, installed_apps, …}`. Both sides use `<data_dir>/apps`, so installed bundles need no migration. Launcher IDs of installed apps become `hub:<manifest-id>`. `home/`'s direct `octosense-appstore` wiring and the `app-appstore` feature are removed. The alternative (keeping `appstore`) discards PR #45 and most of this plan's value.
2. **Bottom navigation:** keep `home/`'s floating navigation on Android and OpenHarmony. Mobile's swipe-start cue is drawn only in the non-floating branch (desktop phone shell, iOS).
3. **Calendar:** a separate follow-up in this repository. Close mobile PRs #10 and #11 with pointers.

Intentionally **not** carried over:

- `home/.github/workflows/runtime.yml`: this repository runs Home CI from `.github/workflows/home.yml`.
- Mobile's `tools/setup-native.py` refactor and `tools/test_setup_native.py`. Its only functional addition (`makepad_override`) was dropped again by mobile PR #48, and `home/runtime-patches.lock.json` is this repository's reviewed equivalent. The test file exercises an API that does not exist here.
- Sibling-checkout paths (`../Octoscript-AppCard-camera`, `../Octosense-Service-AppCards`, `../makepad`): `home/` keeps `../.sources/...`.
- `cfg(any(target_os = "android", target_os = "ios"))` on App Hub code becomes `cfg(native_mobile)`, so OpenHarmony keeps App Hub.

## Validation already done (scratch worktree, 2026-09-25)

With exactly the resolutions below:

- `cargo check --locked --workspace --features mobile-apps` passes, with only the known `app-finance`/`app-robrix` cfg warnings.
- `cargo test --locked --features mobile-apps -p octosense -p octosense-app-policy -p octosense-app-hub -p octosense-app-hub-app -p octosense-maps -p octosense-news -p octosense-appcard`: 588 passed, 0 failed (Home 311, App Hub app 36, Maps 106 including the new TLS tests, News 90, App Hub policy/admission 41, AppCard 3), after the Task 1 Step 4 repair.
- Pre-existing breakage found on this repository's `main`: the News, Maps and AppCard test builds fail with `E0063 missing field 'windows' in InstanceHandles`. The pinned Makepad added that field and CI never builds those crates' tests. Task 1 Step 4 repairs it.
- `python3 -m unittest discover -s tests` passes (27 tests), and `python3 scripts/setup-home.py --check --cargo` passes.
- `Cargo.lock` changes by +21/−4 lines; the only new package is `octosense-app-hub-app`.
- The merge adds 147 files (36.8 MB), mostly `home/scripts/individuals/*.png` and `home/docs/design/app-hub/`.

---

### Task 1: Prepare the branch and fetch mobile

**Files:** none

**Step 1: Start from a clean, current main**

```bash
cd octosense-rom
git status --porcelain            # expect no output
git switch main && git pull --ff-only
git switch -c feature/sync-mobile-main
```

**Step 2: Fetch mobile main**

```bash
git remote add mobile git@github.com:OctoSense-org/OctoSense-mobile.git
git fetch mobile main
git log --oneline -1 mobile/main
```

Expected: `65488bb Merge pull request #50 …`. If mobile `main` has moved, list the new commits with `git log --oneline 65488bb..mobile/main` and review them against this plan before continuing.

**Step 3: Confirm the common base**

```bash
git merge-base HEAD mobile/main
```

Expected: `653cd67b10c938f690933090c9f3f5d78ab91fae`.

**Step 4: Repair the stale module test initializers (pre-existing on `main`)**

The failing check comes first. It reproduces on today's `main`:

```bash
python3 scripts/setup-home.py
(cd home && cargo test --locked -p octosense-maps -p octosense-news -p octosense-appcard --no-run)
```

Expected: FAIL with `error[E0063]: missing field 'windows' in initializer of 'makepad_app_module::InstanceHandles'`.

In the `InstanceHandles { … }` test initializer of each of `home/apps/maps/src/module.rs`, `home/apps/news/src/module.rs` and `home/apps/appcard/src/lib.rs`, add the field that `home/src/module_host.rs` already passes. Put it after `replies,` at the same indentation:

```rust
            windows: Default::default(),
```

Run the tests again, then commit:

```bash
(cd home && cargo test --locked -p octosense-maps -p octosense-news -p octosense-appcard)
git add home/apps/maps/src/module.rs home/apps/news/src/module.rs home/apps/appcard/src/lib.rs
git commit -m "Pass the module windows handle in bundled app tests"
```

Expected before committing: all pass. After the merge, Maps has 106 tests, News 90 and AppCard 3.

### Task 2: Run the subtree merge

**Step 1: Merge without committing**

```bash
git merge --no-ff --no-commit -X subtree=home mobile/main
```

Expected: `Automatic merge failed`, with CONFLICT on exactly these paths:

```
home/.github/workflows/runtime.yml   (modify/delete)
home/Cargo.lock
home/Cargo.toml
home/apps/maps/Cargo.toml
home/src/apps.rs
home/src/main.rs
home/src/mobile_island.rs
home/src/mobile_surface.rs
home/tools/setup-native.py
```

**Step 2: Confirm nothing landed outside `home/`**

```bash
git diff --cached --name-only | grep -v '^home/'
```

Expected: no output.

### Task 3: Mechanical resolutions

**Files:**
- Delete: `home/.github/workflows/runtime.yml`, `home/tools/test_setup_native.py`
- Keep ours: `home/tools/setup-native.py`, `home/src/mobile_island.rs`, `home/Cargo.lock` (regenerated in Task 8)
- Take theirs: `home/apps/maps/Cargo.toml`

**Step 1:**

```bash
git rm -q home/.github/workflows/runtime.yml
git rm -q -f home/tools/test_setup_native.py
git checkout --ours home/tools/setup-native.py home/src/mobile_island.rs home/Cargo.lock
git checkout --theirs home/apps/maps/Cargo.toml
git add home/tools/setup-native.py home/src/mobile_island.rs home/Cargo.lock home/apps/maps/Cargo.toml
```

`apps/maps/Cargo.toml` differs between the sides only by mobile's added Android rustls/tokio dependencies and dev-dependencies. Both sides already carry the same makepad pins.

### Task 4: `home/Cargo.toml` (App Hub dependency)

**Step 1: Resolve the three conflict hunks**

- Hunk 1 (optional `octosense-appstore` dependency, ours) → delete it (take theirs, which is empty).
- Hunk 2 (native-mobile target dependencies) → exactly:

```toml
octosense-app-hub-app = { path = "apps/app-hub" }
octosense-camera = { path = "../.sources/camera/apps/camera/native" }
```

- Hunk 3 (features) → exactly:

```toml
mobile-apps = ["app-reference", "app-sheets", "app-photos", "app-appcard", "app-mail", "app-news", "app-maps", "app-camera", "app-hub"]
app-hub = ["dep:octosense-app-hub-app"]
```

**Step 2: Remove what auto-merged from ours but belongs to the removed wiring**

- Under `[target.'cfg(any(target_os = "android", target_os = "ios", target_env = "ohos"))'.dependencies]`, delete `octosense-appstore = { git = "https://github.com/OctoSense-org/OctoSense-App-Hub.git", rev = "97c2a1f…" }`. Keep the `target_env = "ohos"` condition.
- Delete the feature line `app-appstore = ["dep:octosense-appstore"]`.

**Step 3: Verify**

```bash
grep -n 'appstore\|app-hub\|^default\|^members\|\.\./Octo\|\.\./makepad\|\[patch' home/Cargo.toml
```

Expected: no `appstore` lines. `octosense-app-hub-app` appears twice (optional, and under the native-mobile target). The file also has `default = ["app-hub"]`, `app-hub = [...]`, a `members` list ending in `"apps/app-hub"`, no `../Octo…` or `../makepad` paths, and `[patch.crates-io]` with the `nix` pin still present.

**Step 4: Mark resolved**

```bash
git add home/Cargo.toml
```

### Task 5: `home/src/apps.rs` (installed-app identities)

Keep this repository's structure: `bundled_modules_catalog()` is used by `clients.rs` and `octosense/catalog.rs` to stop installed apps from shadowing built-ins. Take mobile's `hub:` identity functions.

**Step 1: Resolve the five hunks**

- Hunk 1 (module list tail + `installed_card_apps`) → exactly:

```rust
    #[cfg(any(feature = "app-hub", native_mobile))]
    {
        out.push(&octosense_app_hub_app::APP_HUB_MODULE);
        out.push(&octosense_app_hub_app::CARD_MODULE);
    }
    out
}

/// Manifest IDs live in a separate namespace from built-ins and user catalog
/// entries (catalog IDs cannot contain a colon).
pub fn installed_launch_id(manifest_id: &str) -> String { format!("hub:{manifest_id}") }
pub fn card_manifest_id(app: &crate::clients::AppDef) -> Option<&str> {
    (app.bin == "card").then(|| app.id.strip_prefix("hub:")).flatten()
}

pub fn matches_running_app(app: &crate::clients::AppDef, running_id: &str, title: &str) -> bool {
    if app.bin == "card" || running_id.starts_with("hub:") { running_id == app.id }
    else { crate::clients::word_match(running_id, &app.id) || crate::clients::word_match(title, &app.id) }
}

/// Card apps App Hub installed: each is an app of its own in the launcher,
/// hosted by the linked `card` module under its `hub:<manifest-id>` identity.
/// Read fresh each time, so an install shows up without a restart.
pub fn installed_card_apps() -> Vec<crate::clients::AppDef> {
    #[cfg(any(feature = "app-hub", native_mobile))]
    if let Some(root) = octosense_app_hub_app::data_root_if_set() {
        return octosense_app_hub_app::installed_apps(&root).into_iter()
            .map(|app| crate::clients::AppDef {
                id: installed_launch_id(&app.id), label: app.name, bin: "card".into(),
                package: String::new(), dir: String::new(), manifest: None,
                args: Vec::new(), policy: crate::clients::LaunchPolicy::OrFocus,
            }).collect();
    }
```

(The existing `    Vec::new()\n}` after the hunk closes the function.)

- Hunk 2 (`bundled_catalog` / `bundled_modules_catalog` split) → ours.
- Hunk 3 (`AppRegistry::module`) → ours.
- Hunk 4 (`hosting`) → exactly:

```rust
        if matches!(id, "robrix" | "finance" | "apphub") && self.module(id).is_some() && !self.overrides.contains_key(id) {
            return Hosting::Module;
        }
        // An installed card app has no process form anywhere: the `card`
        // module hosts it on every platform, no switch needed.
        if installed_card_apps().iter().any(|app| app.id == id) && self.module("card").is_some() {
```

- Hunk 5 (linked-module test expectation) → exactly:

```rust
                   ["reference", "sheets", "photos", "appcard", "mail", "news", "maps", "camera", "apphub"]);
```

**Step 2: Undo the auto-merged double listing**

In `bundled_modules_catalog()`, the merge takes mobile's `}).chain(installed_card_apps()).collect()`. Change it back to `}).collect()`. `bundled_catalog()` already appends installed apps, and `clients::merge_catalog` must receive only built-ins as its native list.

**Step 3: Verify**

```bash
grep -n 'chain(installed_card_apps\|octosense_appstore\|app-appstore\|"appstore"' home/src/apps.rs
```

Expected: no output. Mobile's test `installed_card_identity_never_focuses_a_builtin_with_the_same_name` is present.

**Step 4: Mark resolved**

```bash
git add home/src/apps.rs
```

### Task 6: `home/src/main.rs`, `style.rs`, `launcher.rs` (App Hub wiring)

**Step 1: Resolve the five `main.rs` hunks**

- Hunks 1–4 (`launch_module_as` call and signature, `configured_open` for `card`, `ClientSlot::module`) → theirs. Mobile passes the `&clients::AppDef` through, and `card` opens with `apps::card_manifest_id(app)`. This removes `launch_module` and `card_app_open`, which have no other callers.
- Hunk 5 (widget action handling) → exactly:

```rust
            #[cfg(any(feature = "app-hub", native_mobile))]
            match wa.cast::<octosense_app_hub_app::AppHubAction>() {
                octosense_app_hub_app::AppHubAction::Launch(id) => self.launch_app(cx, &id),
                octosense_app_hub_app::AppHubAction::OpenInstalled(id) => self.launch_app(cx, &apps::installed_launch_id(&id)),
                octosense_app_hub_app::AppHubAction::None => {}
```

**Step 2: Delete the leftovers of the old path in `launch_module_as`**

Remove these two blocks, which auto-merged from ours:

```rust
        let (card_open, client_label) = match card_app {
            Some((open_json, app_id, label)) => (Some(schema.validate(&open_json, &[])), Some((app_id, label))),
            None => (None, None),
        };
```

```rust
        let (slot_app, slot_label): (&str, &str) = match &client_label {
            Some((app_id, label)) => (app_id.as_str(), label.as_str()),
            None => (module.id(), module.label()),
        };
```

**Step 3: Keep one data-root setup**

The merge contains both mobile's `octosense_app_hub_app::set_data_root(...)` at the top of `handle_startup` and ours after `AppRegistry::load`. They set the same `<data_dir>/apps` path. Delete ours:

```rust
        #[cfg(any(feature = "app-appstore", native_mobile))]
        octosense_appstore::set_data_root(
            cx.get_data_dir().map(|dir| std::path::PathBuf::from(dir).join("apps"))
                .unwrap_or_else(|| octosense::paths::home().join("apps")),
        );
```

**Step 4: Make App Hub cfgs include OpenHarmony**

```bash
sed -i '' \
  -e 's/#\[cfg(any(feature = "app-hub", target_os = "android", target_os = "ios"))\]/#[cfg(any(feature = "app-hub", native_mobile))]/g' \
  -e 's/#\[cfg(not(any(feature = "app-hub", target_os = "android", target_os = "ios")))\]/#[cfg(not(any(feature = "app-hub", native_mobile)))]/g' \
  home/src/main.rs home/src/octosense/style.rs
```

**Step 5: Drop the old launcher icon mapping**

In `home/src/shell/launcher.rs`, delete `"appstore" => Ico::Globe,`. Mobile's `"apphub" => Ico::Menu,` stays.

**Step 6: Verify**

```bash
grep -rn 'octosense_appstore\|app-appstore\|card_app_open\|client_label\|slot_app' home/src
grep -rn 'feature = "app-hub", target_os' home/src
```

Expected: no output from either command.

**Step 7: Mark resolved**

```bash
git add home/src/main.rs home/src/octosense/style.rs home/src/shell/launcher.rs
```

### Task 7: `home/src/mobile_surface.rs` (navigation + swipe cue)

**Step 1: Resolve the single hunk** → exactly:

```rust
        if crate::mobile_navigation::ENABLED {
            self.draw_app_navigation(cx,state,screen);
        } else {
            let bottom=rect(screen.pos.x,screen.pos.y+screen.size.y-24.0,screen.size.x,24.0);
            if phone.screen==PhoneScreen::App || phone.keyboard>0.5 {
                let band=if android {rect(bottom.pos.x,bottom.pos.y,bottom.size.x,bottom.size.y+phone.insets.bottom)} else {bottom};
                self.rounded(cx,band,0.0,if state.style.dark {rgb(28,28,31)}else{rgb(244,244,248)});
            }
            let nav_ink=if phone.screen==PhoneScreen::App || phone.keyboard>0.5 {
                if state.style.dark {rgb(238,238,242)}else{rgb(30,30,34)}
            }else if phone.screen==PhoneScreen::Drawer && !state.style.dark {rgb(30,30,34)}else{rgb(255,255,255)};
            // Floating navigation replaces this band on Android and OpenHarmony.
            // Elsewhere, mark the shell's swipe-start band and keep it after the
            // hints are learned. The keyboard excludes shell swipes, so it hides the cue.
            if phone.keyboard<=0.5 {
                let cue=rect(bottom.pos.x+bottom.size.x*0.5-14.0,bottom.pos.y,28.0,20.0);
                let cue_ink=if phone.screen==PhoneScreen::Home {ink}else{nav_ink};
                self.rounded(cx,cue,10.0,alpha(cue_ink,0.12));
                self.d.icon_centered(cx,Ico::ChevronUp,cue,12.0,alpha(cue_ink,0.90));
            } else if !(android && phone.insets.bottom>0.0) {
                self.rounded(cx,rect(bottom.pos.x+bottom.size.x*0.5-60.0,bottom.pos.y+12.0,120.0,4.0),2.0,nav_ink);
            }
            self.hits.push((bottom,PhoneHit::Home));
            if !ios && phone.keyboard>0.5 {
                let back=rect(bottom.pos.x+12.0,bottom.pos.y-10.0,40.0,34.0);
                self.d.icon_centered(cx,Ico::ChevronLeft,back,16.0,nav_ink);self.hits.push((back,PhoneHit::Back));
            }
```

The `        }` that follows the hunk closes the `else`. Mobile's `mobile_hints.rs`, `shell/ui.rs` (`Ico::ChevronUp`) and `resources/icons/chevron-up.svg` changes auto-merge. Then mark it resolved:

```bash
git add home/src/mobile_surface.rs
```

**Step 2: Verify no markers remain anywhere**

```bash
git diff --name-only --diff-filter=U; grep -rn '^<<<<<<<\|^>>>>>>>' home | head
```

Expected: no output.

### Task 8: Lock file, full verification, merge commit

**Step 1: Update `Cargo.lock` minimally** (`cargo metadata` adds missing entries without upgrading others)

```bash
python3 scripts/setup-home.py
(cd home && cargo metadata --format-version 1 --features mobile-apps > /dev/null)
git diff --stat -- home/Cargo.lock
git diff -- home/Cargo.lock | grep -E '^[-+]name = '
```

Expected: about +21/−4, and the only added package is `+name = "octosense-app-hub-app"`.

**Step 2: Run the CI gates plus the imported crates' tests**

```bash
(cd home && cargo check --locked --workspace --features mobile-apps)
python3 scripts/setup-home.py --check --cargo
(cd home && cargo test --locked --features mobile-apps -p octosense -p octosense-app-policy -p octosense-app-hub -p octosense-app-hub-app -p octosense-maps -p octosense-news -p octosense-appcard)
(cd home && cargo test --locked -p makepad-widgets splash_policy && cargo test --locked -p makepad-script-std gate::tests)
python3 -m unittest discover -s tests -v
```

Expected: all pass: 588 Rust tests in the first command and 27 Python tests. The only compile warnings are the pre-existing `app-finance`/`app-robrix` cfg ones.

**Step 3: Commit the merge**

```bash
git add -A home
git commit -F - <<'EOF'
Merge OctoSense-mobile main (65488bb) into home

Carries mobile PRs #44-#50 after the 653cd67 import: native App Hub
(apps/app-hub, hub:<id> identities, verified installs and updates), Maps
directions on Android 9 over rustls, the swipe-start cue, generated
portraits and App Hub design records. Runtime and AppCard pins were
already mirrored here.

Resolutions: App Hub replaces the direct octosense-appstore wiring and
keeps native_mobile cfgs so OpenHarmony still links it; floating
navigation stays on Android/OpenHarmony and the swipe cue is drawn in
the non-floating branch; product .sources paths, setup-native.py and
root CI are kept; mobile's runtime.yml and setup-native tests are not
imported.
EOF
```

### Task 9: CI, test naming and docs (follow-up commit)

**Files:**
- Modify: `.github/workflows/home.yml` (the `Test Home, App Hub admission and runtime policy` step)
- Modify: `home/src/clients.rs` (catalog-merge test fixture ids)
- Modify: `docs/home-migration.md`, `docs/home-build.md`

**Step 1: Test the imported crates in CI.** Change the first `cargo test` line to:

```yaml
          cargo test --locked --features mobile-apps -p octosense -p octosense-app-policy -p octosense-app-hub -p octosense-app-hub-app -p octosense-maps -p octosense-news -p octosense-appcard
```

This also stops the Task 1 Step 4 breakage from coming back unnoticed. Besides the App Hub app and Maps, CI now also tests `octosense-news` and `octosense-appcard`.

**Step 2:** In `home/src/clients.rs`, rename the catalog-merge test's `"appstore"` fixture ids to `"apphub"`. They describe the same shadowing rule; only the name changes.

**Step 3:** In `docs/home-migration.md`, add a section “Second sync from mobile (2026-09-25)”. It should record the 65488bb merge, the decisions above, PR #10's `78e192c`, and the dispositions of PR #11 and `45dbbfb`. Update the “Work preserved outside this migration” table to match.

**Step 4:** In `docs/home-build.md`, change “App Hub” references that imply `octosense-appstore` wiring to the in-tree `apps/app-hub` crate and its default `app-hub` feature.

**Step 5: Re-run Task 8 Step 2, then commit**

```bash
git add .github/workflows/home.yml home/src/clients.rs docs/home-migration.md docs/home-build.md
git commit -m "Test App Hub and Maps crates in CI; record the second mobile sync"
```

### Task 10: Port PR #10's missing commit

**Step 1:**

```bash
git fetch mobile ohos/mate70-air
git cherry-pick -x -X subtree=home 78e192c
```

Expected: applies cleanly (one file, `home/src/mobile_app.rs`).

**Step 2:** Re-run `(cd home && cargo check --locked --workspace --features mobile-apps)`. Expected: passes.

### Task 11: Device validation on the OnePlus 6T (Android 9)

**Step 1: Build and install the standalone pair** (keeps app data; the signer matches the installed Home)

```bash
scripts/build-home.sh --variant standalone --development \
  --sdk /path/to/makepad-android \
  --android-sdk /path/to/android-sdk \
  --gradle-home /path/to/gradle-8.11.1 \
  --java-home /path/to/full-jdk \
  --packager /path/to/cargo-makepad
adb -s <serial> install -r out/home/standalone/OctoSenseHome.apk
adb -s <serial> install -r out/home/standalone/OctoSenseBridge.apk
```

(`--packager` is required: without it the script builds the pinned `cargo-makepad` with `--locked`, and makepad's `.gitignore` excludes `Cargo.lock`.)

**Step 2: Check on the device**, with a screenshot for each item in `out/device-validation/`:

- Home starts; `adb logcat -b crash` is empty and there are no `panicked` lines.
- App Hub opens from the launcher and loads the live catalog. Preview catalog opens a built-in.
- Maps: request directions. The route loads on Android 9 (PR #47's fix).
- Floating navigation still works in an app; no swipe cue is drawn on Android.
- Photos library is intact (data retained).

**Step 3:** Build the ROM variant (`--variant rom`, platform key) to confirm it still builds. Flashing is not part of this plan.

### Task 12: Pull request

```bash
git push -u origin feature/sync-mobile-main
gh pr create --title "Sync OctoSense-mobile main into Home" --body-file <(…summary of Findings/Decisions/validation…)
```

### Task 13: Prepare mobile for archiving (outward-facing; confirm each step with the owner)

1. Comment on and close mobile PR #10: “Ported to octosense-rom (commits …); the rest was already in `home/`.”
2. Comment on and close mobile PR #11, pointing to the Calendar follow-up issue in this repository.
3. Settle the uncommitted notes in the mobile checkout (`findings.md`, `progress.md`, `task_plan.md`).
4. Add a README notice in mobile: “Moved to OctoSense-org/octosense-rom `home/`”.
5. Archive `OctoSense-org/OctoSense-mobile` on GitHub.

### Task 14 (follow-up, separate plan): Calendar module

Port mobile PR #11's Calendar hosting (`3b33910`, `09a134a`): an `app-calendar` feature pointing at a pinned checkout of OctoScript-App-Design-Flow's `apps/calendar/native`, catalog and host wiring, and device validation. Track it as its own issue and plan.
