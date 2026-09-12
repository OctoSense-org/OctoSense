#!/usr/bin/env python3
"""Adapt the DevEco project `cargo makepad ohos deveco` generated for MakeOS.

Usage: ohos_project.py <project dir> [--deveco-home DIR]

Idempotent: `cargo makepad ohos deveco` recreates the project from makepad's
template, so scripts/ohos.sh runs this after every generation and before
every build. Environment:

  MAKEOS_OHOS_BUNDLE   bundle name to publish under; the signing profile must
                       be bound to it (default: keep the generated
                       dev.makepad.makeos)
  MAKEOS_OHOS_SIGNING  a DevEco build-profile.json5 (its app.signingConfigs
                       is copied) or a bare JSON array of signingConfigs, as
                       DevEco's "Automatically generate signature" wrote them.
                       Without it hvigor emits an unsigned HAP only.
  MAKEOS_OHOS_LABEL    launcher label (default MakeOS)

The edits are textual so the template's comments survive; the files are
JSON5, which the json module cannot round-trip.
"""

import argparse
import json
import os
import re
import sys


def read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def write(path, text):
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)


def json5_load(text):
    """Enough JSON5 for DevEco's files: // comments and trailing commas."""
    text = re.sub(r"//[^\n]*", "", text)
    text = re.sub(r",(\s*[}\]])", r"\1", text)
    return json.loads(text)


def deveco_sdk_version(deveco_home):
    """`<platform>(<api>)`, the compatibleSdkVersion form hvigor wants for a
    HarmonyOS product, read from the SDK DevEco bundles."""
    pkg = os.path.join(deveco_home, "sdk", "default", "sdk-pkg.json")
    try:
        data = json5_load(read(pkg))["data"]
        return f'{data["platformVersion"]}({data["apiVersion"]})'
    except (OSError, KeyError, ValueError):
        return None


def set_bundle(prj, bundle):
    path = os.path.join(prj, "AppScope", "app.json5")
    text = read(path)
    new = re.sub(r'"bundleName"\s*:\s*"[^"]*"', f'"bundleName": "{bundle}"', text, count=1)
    write(path, new)
    print(f"  bundleName -> {bundle}")


def set_labels(prj, label):
    """The app label (AppScope) and the ability label (module) both show in
    launchers; the template says "entry" and the crate name respectively."""
    targets = [os.path.join(prj, "AppScope", "resources", "base", "element", "string.json")]
    for locale in ("base", "en_US", "zh_CN"):
        targets.append(os.path.join(prj, "entry", "src", "main", "resources", locale, "element", "string.json"))
    for path in targets:
        if not os.path.isfile(path):
            continue
        text = read(path)
        text = re.sub(r'("name"\s*:\s*"(?:app_name|EntryAbility_label)"\s*,\s*"value"\s*:\s*)"[^"]*"',
                      lambda m: f'{m.group(1)}"{label}"', text)
        write(path, text)
    print(f"  labels -> {label}")


def set_permissions(prj):
    """The template declares no permissions; without INTERNET every network
    call fails silently."""
    path = os.path.join(prj, "entry", "src", "main", "module.json5")
    text = read(path)
    if '"requestPermissions"' in text:
        print("  requestPermissions already present")
        return
    block = (
        '    "requestPermissions": [\n'
        '      { "name": "ohos.permission.INTERNET" },\n'
        '      { "name": "ohos.permission.GET_NETWORK_INFO" }\n'
        '    ],\n'
    )
    new, n = re.subn(r'(\n\s*"pages"\s*:)', "\n" + block.rstrip("\n") + r"\1", text, count=1)
    if n != 1:
        sys.exit("module.json5: could not find the \"pages\" key to insert permissions before")
    write(path, new)
    print("  requestPermissions -> INTERNET, GET_NETWORK_INFO")


def set_launch_parameters(prj):
    """`aa start ... --ps makepad.NAME VALUE` -> environment variables in the
    app (the platform reads the glue's `launchParameters`); the template
    forwards nothing."""
    ability = os.path.join(prj, "entry", "src", "main", "ets", "entryability", "EntryAbility.ets")
    text = read(ability)
    if "launchParameters" not in text:
        anchor = "    let x = ArkGlue.initInstance(this.context);\n"
        if anchor not in text:
            sys.exit("EntryAbility.ets: could not find the ArkGlue.initInstance line")
        forward = (
            "    const parameters: Record<string, string> = {};\n"
            "    for (const key of Object.keys(want.parameters ?? {})) {\n"
            "      const value = want.parameters?.[key];\n"
            "      if (key.startsWith('makepad.') && typeof value === 'string') {\n"
            "        parameters[key] = value;\n"
            "      }\n"
            "    }\n"
            "    x.launchParameters = JSON.stringify(parameters);\n"
        )
        write(ability, text.replace(anchor, anchor + forward, 1))
    glue = os.path.join(prj, "entry", "src", "main", "ets", "makepad", "makepad.ets")
    text = read(glue)
    if "launchParameters" not in text:
        new, n = re.subn(r"(\n\s*filesDir\s*:\s*string;)", r"\1\n  launchParameters: string = '{}';", text, count=1)
        if n != 1:
            sys.exit("makepad.ets: could not find the ArkGlue filesDir field")
        write(glue, new)
    print("  launch parameters (makepad.*) forwarded to the app")


def set_xcomponent_library(prj):
    """The XComponent loads lib<libraryname>.so for its native surface; the
    Rust side is staged as libmakepad.so, the template still says 'entry'."""
    path = os.path.join(prj, "entry", "src", "main", "ets", "pages", "Index.ets")
    text = read(path)
    new = re.sub(r"libraryname\s*:\s*'entry'", "libraryname: 'makepad'", text)
    write(path, new)
    print("  XComponent libraryname -> makepad")


def load_signing_configs(source):
    data = json5_load(read(source))
    if isinstance(data, dict):
        data = data["app"]["signingConfigs"]
    if not isinstance(data, list) or not data:
        sys.exit(f"{source}: no signingConfigs found")
    return data


def set_build_profile(prj, sdk_version, signing):
    path = os.path.join(prj, "build-profile.json5")
    text = read(path)
    if sdk_version:
        text = re.sub(r'"compatibleSdkVersion"\s*:\s*"?[^",\n]*"?',
                      f'"compatibleSdkVersion": "{sdk_version}"', text, count=1)
        text = re.sub(r'^\s*"compileSdkVersion"\s*:\s*[^,\n]*,?\n', "", text, flags=re.M)
    text = text.replace('"runtimeOS": "OpenHarmony"', '"runtimeOS": "HarmonyOS"')
    if signing:
        body = json.dumps(signing, indent=2)
        body = "\n".join("    " + line for line in body.splitlines())
        text, n = re.subn(r'"signingConfigs"\s*:\s*\[[^\]]*\]', f'"signingConfigs": {body.lstrip()}', text, count=1)
        if n != 1:
            sys.exit("build-profile.json5: could not replace signingConfigs")
        if '"signingConfig"' not in text:
            # The product's reference (an unsigned build before this one
            # stripped it). Anchor on the products block: the signing entry
            # spliced in above is also named "default" and comes first.
            text, n = re.subn(r'("products"\s*:\s*\[\s*\{\s*"name"\s*:\s*"default",)',
                              r'\1\n        "signingConfig": "default",', text, count=1)
            if n != 1:
                sys.exit("build-profile.json5: could not find products[0] to reference the signing config")
    else:
        # No material: drop the product's reference so hvigor emits the
        # unsigned HAP instead of failing on an empty signing config.
        text = re.sub(r'^\s*"signingConfig"\s*:\s*"[^"]*",?\n', "", text, count=1, flags=re.M)
    write(path, text)
    print(f"  build-profile: compatibleSdkVersion={sdk_version or '(kept)'} runtimeOS=HarmonyOS "
          f"signing={'yes (' + signing[0].get('type', '?') + ')' if signing else 'none, unsigned HAP'}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("project")
    ap.add_argument("--deveco-home", default=os.environ.get("DEVECO_HOME", "/Applications/DevEco-Studio.app/Contents"))
    args = ap.parse_args()
    prj = os.path.abspath(args.project)
    if not os.path.isfile(os.path.join(prj, "build-profile.json5")):
        sys.exit(f"{prj}: not a DevEco project (run `scripts/ohos.sh deveco` first)")
    print(f"adapting {prj}")
    bundle = os.environ.get("MAKEOS_OHOS_BUNDLE")
    if bundle:
        set_bundle(prj, bundle)
    set_labels(prj, os.environ.get("MAKEOS_OHOS_LABEL", "MakeOS"))
    set_permissions(prj)
    set_xcomponent_library(prj)
    set_launch_parameters(prj)
    signing_src = os.environ.get("MAKEOS_OHOS_SIGNING")
    signing = load_signing_configs(signing_src) if signing_src else None
    set_build_profile(prj, deveco_sdk_version(args.deveco_home), signing)
    local = os.path.join(prj, "local.properties")
    if os.path.exists(local):
        os.remove(local)


if __name__ == "__main__":
    main()
