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
