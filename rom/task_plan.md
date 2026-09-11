# MakeOS scoping plan

## Objective
Implement the agreed minimal standalone MakeOS project derived from Makepad's `apps/wm`, runnable with `cargo run`, with process hosting and explicit tracking/sync of copied upstream files. User approved the proposed first milestone.

## Phases
1. Inspect destination, source app, repository state, and instructions — complete.
2. Trace crate dependencies, runtime assets, platform assumptions, and app-hosting mechanisms — complete.
3. Compare extraction and upstream-sync approaches; define implementation scope and acceptance criteria — complete.
4. Document the proposed plan and discuss consequential open decisions with the user — complete; user approved.
5. Import source with provenance and resolve pinned external dependencies — complete.
6. Adapt desktop resources, startup, state, and app catalog; add reference app — complete; review fixes verified.
7. Implement and test upstream maintenance workflow — complete; 24 maintenance fixtures passed.
8. Run relevant tests and verify plain cargo run plus hosted app interaction — complete; root and independent-copy smoke checks passed.
9. Review, document, and finalize runnable project — complete; final inventory and whitespace check passed.
10. Design one-command daily sync using the updated local Makepad HEAD — complete.
11. Add regression tests and implement cached staging, automatic verification, reports, and review-branch handoff — complete.
12. Verify no-op, success, conflict, failed-check, and concurrent-edit behavior; document the daily command — complete; 40 script tests and full native verifier passed.

13. Resolve upstream ae20efc5 conflicts and verify the standalone upgrade — complete; all checks passed and the verified candidate is ready for the review-branch handoff.

14. Merge completed sync into main and adopt the fork WM features — complete; main advanced to 8b2dc9c before import, fork features integrated with 205 Rust and 44 Python tests plus native glass/style and cargo run validation. See `docs/plans/2026-09-08-fork-wm-features.md`.

15. Restore the Omarchy startup wallpaper — complete; bundled the original default image with provenance, retained installed-image precedence and opt-in downloads, and verified offline startup plus all style transitions.

16. Reuse the fork’s installed Qwen model — complete; linked the existing GGUF into MakeOS state and verified hosted local inference. Outer-pane input routing remains a separate unverified observation, documented with the test evidence.

17. Document local AI installation for other users — complete; per-user storage, pinned verified download, existing-model reuse and model ignore rules documented and checked.

## Daily sync requirements (2026-09-06)
- User performs the source Git pull/fetch; the command uses local HEAD by default and never pulls or commits.
- Automate comparison, safe staged merging, coordinated pins/lockfile, compile/tests, release build, and both native smoke modes.
- No new revision means a fast no-op. Real updates require a clean MakeOS tree.
- Reuse an ignored staging build cache; serialize syncs to prevent cache interference.
- Retain comparison, verification logs, smoke artifacts, and failure candidates for review.
- Create a dedicated review branch only after successful checks. User owns review, commit, merge, and push.

## Constraints
- Minimize copied code; prefer external crate dependencies where practical.
- Track exact upstream provenance and deliberate local changes.
- Do not modify the Makepad checkout.
- Implementation and GUI verification are authorized by the user's approval.
- Destination is `/Users/guofoo/git/mp/makeos`; interpret this as the project root unless clarified.

## Agreed defaults
- macOS first; retain platform branches without promising untested targets.
- Process-hosting reference app; keep optional module infrastructure.
- Pinned Git crates, lean default app/services set, separate MakeOS state.

## Proposed plan
`docs/plans/2026-09-04-makeos-extraction.md` contains the implementation sequence, sync strategy, agreed assumptions, and acceptance criteria.

## Errors and limitations
- Initial `rg --files` returned exit 1 because the destination is empty.
- The initial combined source AGENTS.md read was truncated; inspect relevant sections separately as needed.
- Some exploratory queries were overbroad and truncated; subsequent source reads use bounded ranges.
- `config/omarchy` does not exist; source references found so far are comments, not runtime loads.
- A resource search included nonexistent `platform/derive`; useful matches were returned from the real paths, and no dependency on that directory was assumed.
- Git initialization and dependency fetching required sandbox escalation; both completed after approval.
- Imported tests assuming bundled app registry were adapted to explicit fixture/catalog entries. Shutdown test needed a condition-based wait for grandchild exit.
- Code review found final host shutdown cleanup, process-group escalation, and asynchronous startup error reporting gaps; corrected and verified with regression tests and native smoke checks.
- Runtime inspection exposed stale run-view widget geometry after the startup background disappears; explicitly selecting the live surface area fixed it.

## Official work update — 2026-09-11
18. Snapshot existing local changes and compare official work, including wm_api/wm_theme — complete.
19. Resolve fork-specific API/style compatibility, integrate all WM changes and crate updates in an isolated candidate — complete.
20. Verify tests, desktop GPU hosting/styles and Android builds; update provenance/docs and apply reviewed candidate without losing local edits — complete (iOS upstream failures and unavailable ADB device documented).

## OctoSense rename
21. Inventory project identifiers, state compatibility, packaging and provenance — complete.
22. Rename active code, Cargo targets, resources, scripts and documentation; retain historical provenance and compatibility — complete.
23. Verify Cargo/tests, desktop branding and hosting, Android package metadata, and sync integrity — complete (no ADB device attached).
