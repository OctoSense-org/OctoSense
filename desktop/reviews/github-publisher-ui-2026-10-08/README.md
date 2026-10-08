# Keyless publisher UI acceptance — 2026-10-08

[简体中文](README.zh-CN.md)

The final packaged macOS shell at `5e1a8414cba4c9c3ab041b5555b6d90bcbad03bb` passed **8/8 checks** in a new native Makepad instrument run with fresh app data. Its [receipt](macos-final-5e1.json) binds the executable and genuine GitHub release packs by SHA-256. The earlier source-`5607e90b` [receipt](macos-5607.json) and captures below remain unchanged; the rebuilt executable has identical bytes, but the final package was actually rerun. This is a focused publishing/install/update test, not extended UX soaking or a release announcement.

The run declined App Hub's optional agent, searched for Publishing Test Notes, installed and opened version 0.1.0, added a note, restarted, updated to 0.1.1, retained that note, added another and removed the first, then reopened the installed app from Library after another restart. The remaining note persisted. All owned shell processes stopped. Four original captures were inspected; no personal accounts, credentials, or private paths appear in them.

| Captured state | Evidence |
| --- | --- |
| Search result | [Search](01-search.png) |
| Installed app with its first note | [Version 0.1.0](02-installed-note.png) |
| Updated app after editing its notes | [Version 0.1.1](03-updated-notes.png) |
| Reopened app with retained data | [Library reopen](04-retained-note.png) |

## Proof and catalog boundaries

The genuine developer proofs came from the fixture's [0.1.0 workflow](https://github.com/ymote/octosense-publisher-fixture/actions/runs/37736273522) and [0.1.1 workflow](https://github.com/ymote/octosense-publisher-fixture/actions/runs/37736765473). No developer signing key or repository signing secret was used. The shell installed the sealed release bytes, not a restamped source checkout.

Admission used an **isolated local legacy test catalog**, signed with temporary Hub-only keys that were deleted after both catalog snapshots were prepared. It explicitly selected `OCTOSENSE_HUB_CATALOG=legacy` with new shell, app, and core data directories. It did not mutate the public catalog or submit the fixture to App Hub. This is separate from verification of the production GitHub-attested sequence-11 catalog. A real developer requests publication through an [App Hub submission issue](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/new?template=submit-app.yml); a successful GitHub release alone does not request or approve admission.

## Instrument limitations

Earlier attempts exposed missing driver handling for first-run agent consent, scrolling to the install confirmation, and startup snapshot readiness. They also encountered transient `wait=1` frame-acknowledgement errors after input had already applied. The successful driver queued native input with `wait=0`, then checked the actual UI and stored state; it did not replay ambiguous writes. Read-only screenshot arming retries were bounded to eight seconds. The receipt records these limits and earlier outcomes rather than treating every attempt as a pass.

The custom-drawn optional-agent consent buttons were not in the widget snapshot. The driver used the inspected native screen to choose **Don't allow** and verified the denied consent in its own profile. No agent or model call was required for the tested app.

## Android fixture preparation

The original two packs declare only macOS. New genuine [0.1.2](https://github.com/ymote/octosense-publisher-fixture/actions/runs/37742612574) and [0.1.3](https://github.com/ymote/octosense-publisher-fixture/actions/runs/37742627927) releases declare macOS and Android and use the merged Design Flow generator at `6f4ce207` with App Hub `655114c4943cd2490daaefa2173e7b5aaa20669f`. Their workflows, native proof verification, platform declarations, and hashes passed; see the [preparation receipt](android-fixture-releases.json). This receipt does **not** claim phone UI acceptance. The old packs and evidence remain unchanged.

## Final package and OnePlus 6

The final Mac run repeated all eight checks. Original [installed](final-01-installed-note.png) and [updated](final-02-updated-notes.png) captures were inspected. [Capture hashes](captures-sha256.json) bind the checked-in originals.

The actual OnePlus 6 run at `8afcf35f03649f31b939b822302c132f18feb86e` passed **8/8 functional checks**: search/install/open 0.1.2, add a note, update to 0.1.3 with exact proof and note retained, edit the note list, and restart/Library Open with the final note intact. See its [receipt](android-8af.json) and [original reopened screen](android-24-library-opened.png). Other capture hashes remain in the receipt; raw logs and private profiles are not committed.

**The phone has a visual defect:** “Publishing Test Notes” clips at the right edge. Body text wraps and the tested actions remain reachable. This is a functional pass, not a clean mobile UX pass. The inherited starter-template heading has a separate narrow-window fix; that fix was not applied to these immutable phone releases or claimed phone-tested.

The phone used an isolated debug Home package, scoped ADB input and an ephemeral local legacy catalog. It did not replace normal Home or use personal accounts/models. Its source predates the final packaging-only changes; the receipt retains the actual source and APK hashes. Both Mac and phone test processes were stopped. These are synthetic publisher-fixture results, not acceptance of the separately submitted ymote Notes, Inbox, Calendar or Camera apps.
