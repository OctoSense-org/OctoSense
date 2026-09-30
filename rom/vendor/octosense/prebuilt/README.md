# Prebuilt APKs

Build the ROM variant with `scripts/build-home.sh --variant rom` using the existing
platform key/certificate, then run `python3 scripts/stage-home.py` from the product
root. It verifies the build receipt and stages `OctoSenseHome.apk` and
`OctoSenseBridge.apk` here before `scripts/apply-to-tree.sh` copies the product layer.
Home is staged without its native libraries: they go to `lib/arm64/`
(`libmakepad.so` and the octos kernel `liboctos.so`), which `../Android.mk`
installs into Home's `lib/arm64` directory on the image, so the kernel is a file
Home can exec.

The APKs and libraries are ignored build artifacts. Android's existing `certificate: "platform"`
imports remain unchanged. Standalone/development build receipts are rejected by
the stager. See [Home builds](../../../docs/home-build.md).
