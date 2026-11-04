# Connected-app previews 0.1.1 — admission record

This records the signed catalog candidate at sequence 10. The three publisher
releases are public; the default Store receives them only after this catalog
change merges. Existing 0.1.0 entries and files, the official anchor/working-key
chain, and publisher key are unchanged.

| App | Change | Submission |
| --- | --- | --- |
| Inbox Assistant | Require the admitted workspace template and initial email data for notifications; remove executable-source alternatives. | [#130](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/130) |
| GitHub Notes | Declare the existing consent-gated foreground read-only Ask agent and its instructions. | [#131](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/131) |
| Google Calendar Sync | Disclose past 30 days / next 366 days when the host reports bounds; preserve an explicit unavailable-range fallback for older hosts. | [#132](https://github.com/OctoSense-org/OctoSense-App-Hub/issues/132) |

[admission.json](admission.json) binds the exact publisher commits, tags, bundle
file hashes, review packets, catalog hash and tooling. All eight publisher
answers were read. The adjacent source-review verdicts are the maintainer
assistant's actual review under publication authorization; no external model
reviewer was invoked. `hub scan` and `hub publish` consumed a reviewer adapter
that returned these verdicts only for the exact hashed packets. No `--reviewed`
bypass was used. Each app directory includes its publish output and repeated
signed-artifact gate.

[remote-downloads.json](remote-downloads.json) records anonymous HTTPS downloads
of every published tar/pack and SHA256SUMS. Decoded files equal the immutable
publisher tag and catalog artifacts; the release pack equals the Rust-generated
catalog pack. Immutable privacy documents also returned HTTP 200.

[remote-store-install.json](remote-store-install.json) records actual Hub
`Remote`, pack unpacking and `Store` execution against the immutable remote
candidate commit. For each app it verified a fresh signed install, upgrade from
0.1.0, retained fictional local state, continued opening of the old installed
version before upgrade, prepared-launch validation, agent-file loading and
reopening through a new Store instance. Owned temporary profiles were removed.
This does **not** start an agent peer or prove native UI, model/provider traffic,
physical approval, Android support or default-main availability.

The three signed source releases passed their focused contract checks (Inbox 4,
Notes 3, Calendar 3); Calendar's exact range helper also passed four native
fixture cases. Original screenshots and historical acceptance remain labelled
as historical. All listings remain macOS developer previews. Calendar's finite
host range is implemented in [OctoSense #356](https://github.com/OctoSense-org/OctoSense/pull/356);
older hosts keep the unavailable-range status.

Tracked-file checks found no private signing-key bytes, private local paths or
credential markers. Optional publisher secret scans are recorded accurately in
`admission.json`: public Git commit IDs produced false positives; an unavailable
or failed optional scan is not presented as a pass. Historical 0.1.0 receipts
were preserved byte for byte.

## Reproduce the Store verification

Prepare the sibling source layout from the repository build guide first. From
the Hub root, copy the recorded helper into a temporary example name that does
not already exist, then run it against the immutable public commits:

```sh
mkdir -p crates/app-hub/examples
if test -e crates/app-hub/examples/verify_release.rs; then
  echo "The temporary example already exists; choose another name."
  exit 1
fi
cp docs/admissions/connected-apps-0.1.1/verify_install.rs crates/app-hub/examples/verify_release.rs
cargo run --locked --release -p octosense-app-hub --example verify_release -- \
  https://raw.githubusercontent.com/OctoSense-org/OctoSense-App-Hub/84c85a86018762d19e32181aed6ee01dee106f66 \
  https://raw.githubusercontent.com/OctoSense-org/OctoSense-App-Hub/a52d928b0c14d2822c09f6ccd18f44bf7b885221 \
  /tmp/connected-apps-install-receipt.json
rm crates/app-hub/examples/verify_release.rs
```

The install policy checks catalog freshness against the execution date. A much
later run may correctly refuse this historical catalog; do not bypass freshness
or silently substitute a newer catalog and label it the same proof.

[中文](README.zh-CN.md)
