# Developer submission preflight — 2026-10-08

This is a structural check of the public submission candidates, not store
admission or a claim that every app works. The [receipt](receipt.json) records
exact source revisions, bundle paths, publisher public keys when supplied,
commands, findings and the gate executable's hash. It covers 31 bundle
candidates and four threads routed separately.

The gate was built from App Hub `59f3743` in [PR #146](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/146).
Every downloaded file was checked against its Git blob hash and size. No
publisher bundle was changed, restamped or signed, and no repository build
script was executed. No catalog entry or publication status changed.

## Results

- **19 primary candidates pass structural checks.** They still need native
  interaction, host-service, consent and publication review.
- **10 primary candidates are refused:** nine digest mismatches and one
  private capability that the public host does not implement.
- **Two stated sources are unavailable.** A separate tag observation is
  recorded for OnCue; it does not replace the unresolved submitted SHA.
- **Four threads take another route:** #52 and #60 are superseded by #77;
  #97 is a native OctoBuddy module routed to OctoSense #344; #110 requests
  native Windows/Rinx extension review, not ordinary bundle admission.

| Issue | Finding | Required next step |
| --- | --- | --- |
| #40, #84, #103, #111 | Submitted bytes do not match the manifest digest. | Publisher rebuilds the final bundle with the current tool, stamps, signs when applicable, and submits an immutable new revision/version. The exact computed and claimed digests are in the receipt. |
| #74, #99, #102, #113, #115 | The claimed digest exactly matches the old Windows backslash-path hash. The current portable hash correctly refuses it. | Publisher stamps with the current `hub`, signs when applicable, commits exact bytes with `.gitattributes`, and checks a fresh checkout. Do not add a legacy-hash bypass. |
| #95 | `liyu` is a private capability. | Migrate the app's account and requests to the declared backend service, or submit the native host extension separately. A public host cannot execute an absent Rust function. |
| #78 | The stated commit does not resolve; `v0.4.8` resolves to a different commit that passes the gate. | Publisher confirms an immutable source identity. Matrix/Rinx functionality still needs its target-host review. |
| #105 | The stated repository/commit returns HTTP 404 to this reviewer. | Publisher makes the exact source accessible. This does not distinguish deletion, renaming or private visibility. |
| #116 | Required font attribution links were incorrectly treated as external asset loads. | PR #146 fixes this host gate defect; the unchanged candidate then passes. Its custom navigation host integrations still need review. |
| #104 | The gate passes, but a clean first use is blocked before the main task. | Add and verify a household-setup flow, described below. |

The receipt preserves the requested refs. For #40 the inspected candidate is
`v0.5.0`, the latest version announced in that issue's comments. For #76 this
check covers the frozen `bundle/` at `v0.3.3`, not its separate moving script
app or native variants. A passing gate does not mean those variants were run.

## Native first-use check

Doudou #104 was launched in release `card-host` with a new, isolated data
directory, on macOS Metal at 412×892 logical points. Codex drove the native
Makepad instrument; no model generated this test or controlled the clicks.
The bundle requested only storage. Household text was fictional.

The app renders Chinese and accepts text. Submitting household information
clears the editor but leaves “请先在终端运行迁移或输入家庭信息”; it creates no
household. In the same source, `g8_init` only restores an existing household
or migration, and `flow_submit_text` refuses a missing household. The view
checks missing household before displaying the error. That prevents a new
installation from reaching the scenario and decision task. Earlier tests
with prepared storage do not establish this first-use path.

![Actual native first-use result](first-use-doudou.png)

The owned process exited. Restart persistence after a successful setup,
Glance transitions, phone behavior and live providers were not verified in
this run. No UX score or publication approval is assigned.

## Host work is separate

Generic app-tool dispatch and backend authentication already exist in
OctoSense source. Availability still depends on the actual packaged host,
provider registration and platform approval support. The compatible release
and physical/native acceptance cannot be inferred from an admitted manifest.

The follow-up work includes App Hub #146 (development checks, fonts and
portable tool CI), App Hub #147 plus OctoSense #368 (media methods),
OctoSense #361 (desktop embedded browsers), and OctoSense #367 (Wasm
isolation). Their PR checks and device receipts own their validation status;
this record does not assert that they have merged or shipped.

Use [the submission guide](../../docs/SUBMITTING.md) for the final byte,
signing and reviewer checks. Keep this dated evidence immutable; put later
runs in a new record.
