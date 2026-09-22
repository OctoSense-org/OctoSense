# Prebuilt APKs

Build the ROM variant with `scripts/build-home.sh --variant rom` using the existing
platform key/certificate, then run `python3 scripts/stage-home.py` from the product
root. It verifies the build receipt and stages `OctoSenseHome.apk` and
`OctoSenseBridge.apk` here before `scripts/apply-to-tree.sh` copies the product layer.

The APKs are ignored build artifacts. Android's existing `certificate: "platform"`
imports remain unchanged. Standalone/development build receipts are rejected by
the stager. See [Home builds](../../../docs/home-build.md).
