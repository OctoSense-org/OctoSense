# Test shared components in the desktop release

[简体中文](released-component-acceptance.zh-CN.md)

`tools/test-released-components.py` tests the **packaged macOS desktop**, using
App Hub's Search → Get → Install screens and its normal installed-app launcher.
It installs `org.ymote.componentdemo.first` and `.second` from the genuine
GitHub-attested rehearsal catalog, then restarts the shell for each app and
checks two component calls, counter state and the app's synthetic HTML note.
One app declares no capabilities; the other declares `wasm` and `storage`.
Neither app uses an account or model. The public catalog is not modified.

This complements `tools/test-shared-components.py`, whose native fixture tests
agent tool dispatch, concurrent instance isolation and immutable component
deduplication. Running the fixture is not evidence that a released shell works.
The released-shell test does not establish live-model behavior or an OS upgrade.

## Release and acceptance order

1. Merge the compatible host changes after their required checks. Create the
   `desktop-v<version>` tag on that reviewed commit. The existing
   `.github/workflows/release-desktop.yml` builds the tag on macOS, Windows and
   Linux, scans the packages and attaches them to a **draft** release. A manual
   run must use that tag and `dry_run=false`; a branch-only dry run is not a
   release. Signing and notarization depend on configured release secrets;
   inspect the signing jobs rather than assuming either occurred.
2. Download the draft's macOS `.app.zip`, platform receipt and `SHA256SUMS`.
   Verify the workflow's source commit and all published asset hashes. Run
   `tools/release-scan.py` on the downloaded package. The local packaging script
   expects its standard `target/release` output; use the workflow's clean build
   environment, without an alternate Cargo target directory or build target.
3. Obtain the genuine signed mirror produced by the protected Hub catalog
   dry-run. It contains `catalog-v2.json` and the exact approved artifacts. Do
   not add a trust-anchor override, fake a proof or substitute a legacy catalog.
4. Run the hidden test with a **new** evidence directory:

   ```sh
   python3 tools/test-released-components.py \
     --app-zip /path/to/OctoSense_<version>_macos_aarch64.app.zip \
     --sha256 <the-archive-entry-from-SHA256SUMS> \
     --tag desktop-v<version> \
     --mirror /path/to/verified-candidate-mirror \
     --out /path/to/new-evidence-directory
   ```

   The driver verifies the archive digest and packaged version, extracts the
   `.app` into that directory and leaves the installed personal OctoSense app
   untouched. It uses a fresh `OCTOSENSE_HOME`, a separate kernel directory,
   file vaults, the real GitHub catalog channel and hidden Makepad instrument
   windows. It records hashes, assertions, logs and frames, then closes each
   owned process. Inspect the frames as well as the JSON receipt.
5. Review all platform artifacts and their distinct test limits. Publish the
   reviewed draft explicitly as a **prerelease** for an RC tag; the workflow
   itself does not set the prerelease flag. No Windows/Linux installation or
   physical-phone result follows from this Mac test.

For developer rehearsal only, replace `--app-zip`, `--sha256` and `--tag` with
`--binary /path/to/octosense`. Its receipt records `release_archive_tested: false`.
No build, tag creation, download or release publication is performed by this
driver. An unexecuted recipe is not a passing acceptance result.
