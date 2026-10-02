# Working on the ROM

Follow the [repository rules](../AGENTS.md),
[Home build contract](docs/home-build.md) and
[product walkthrough](../desktop/docs/code-walkthrough.md).

- Put Android product, permission, sepolicy and privileged-service changes in
  `vendor/octosense/`. Build Home and System Bridge from `../phone/`.
- Preserve the package/signature checks in `AgentPlatformService` and the
  AIDL contract used by Home. Update the corresponding Java clients and
  platform validation fixtures when a contract changes.
- Keep build receipts, staging and signing inputs consistent across
  `build-home.py`, `stage-home.py` and the product makefiles. Keep signing
  material and machine configuration outside the repository.
- Run `python3 -m unittest discover -s tests` from `rom/` for build/staging
  changes and syntax-check changed shell scripts with `bash -n`. Validate
  AIDL changes with `python3 scripts/generate-agent-aidl.py --check --sdk "$ANDROID_HOME"`
  with the configured Android SDK, as described in the README.
- Follow the root's assigned-device rules for installation/flashing. Use the
  README's device checks to verify boot, platform functions and OTA behavior
  before release; retain the resulting validation record.
- Update both README languages when the product layout, artifact contract or
  build/flash sequence changes.
