# Working on the ROM

The [repository rules](../AGENTS.md) apply. Read the
[product source walkthrough](../desktop/docs/code-walkthrough.md) and
[Home build contract](docs/home-build.md) before changing packaging docs.

- `vendor/octosense/` is the Android product/platform layer. Home and the
  System Bridge are built from `../phone/`; do not revive a `home/` source copy.
- The Java/Binder privileged agent is not the octos LLM system agent. Document
  package/signature gates separately from assistant consent/tool policy.
- Keep build, artifact receipt verification, staging, signing, installation,
  flashing and release steps explicit. A successful build is not evidence that
  the image boots or that platform functions work.
- Do not install, flash or publish as part of a documentation review. Device
  work requires the user's assigned device and authorized scope. Keep signing
  material and machine paths outside this repository.
- Run the applicable Python/script checks from the root instructions for code
  changes. For docs, check relative links and source-backed command arguments,
  pair README languages and mark unexecuted commands/device work unverified.
