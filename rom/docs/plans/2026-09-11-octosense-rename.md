# OctoSense rename

Rename the application, Cargo packages, custom desktop style, local module and
resource paths, catalog entries, logs, and current documentation. Keep the
Reference app and all existing behavior. The workspace stays in its current
directory; no remote repository changes are needed.

1. Use OctoSense for product labels, octosense for package/binary/module names,
   octosense-reference for the Reference crate, and OCTOSENSE_HOME for state.
2. Set explicit Android product_name=OctoSense and identifier=dev.makepad.octosense.
3. Preserve existing state and model links by falling back to MAKEOS_HOME and
   ~/.makeos when the new environment/path is absent. Never move personal files.
4. Preserve pristine upstream source names and hashes; only local destinations
   change. Historical plans and validation records keep their original names
   and actual artifact paths, with a historical-record notice.
5. Check locked Cargo metadata, workspace tests, Python sync/smoke tests, native
   all-style hosting and cargo run, and the Android package manifest.
6. Leave changes uncommitted for review.
