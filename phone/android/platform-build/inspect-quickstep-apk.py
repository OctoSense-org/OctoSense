#!/usr/bin/env python3
"""Check a signed native Quickstep candidate's package/manifest boundary.

This packaging check does not prove ROM permission grants, SystemUI binding,
controller correctness, deployment rollback or performance.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess


PACKAGE = "dev.makepad.octosense.quickstep"
BIND_PERMISSION = "dev.makepad.octosense.permission.BIND_HOME_INTEGRATION"


def manifest_tree(text):
    roots, stack = [], []
    for line in text.splitlines():
        element = re.match(r"(\s*)E: ([\w-]+)(?: |$)", line)
        if element:
            indent = len(element[1])
            while stack and stack[-1][0] >= indent:
                stack.pop()
            node = {"tag": element[2], "attrs": {}, "children": []}
            (stack[-1][1]["children"] if stack else roots).append(node)
            stack.append((indent, node))
            continue
        attribute = re.match(r"\s*A: ([\w:]+)(?:\(0x[0-9a-f]+\))?=(.*)", line)
        if attribute and stack:
            raw = attribute[2].split(" (Raw: ", 1)[0]
            if raw.startswith('"'):
                value = json.loads(raw)
            else:
                integer = re.fullmatch(r"\(type 0x[0-9a-f]+\)0x([0-9a-f]+)", raw)
                value = int(integer[1], 16) if integer else raw
            stack[-1][1]["attrs"][attribute[1]] = value
    if len(roots) != 1 or roots[0]["tag"] != "manifest":
        raise ValueError("Expected one binary Android manifest")
    return roots[0]


def descendants(node):
    for child in node["children"]:
        yield child
        yield from descendants(child)


def inspect(manifest, signer_output, expected_certificate):
    failures = []

    def require(condition, message):
        if not condition:
            failures.append(message)

    require(manifest["attrs"].get("package") == PACKAGE, "Wrong APK package")
    applications = [n for n in manifest["children"] if n["tag"] == "application"]
    require(len(applications) == 1, "Expected one application")
    if len(applications) != 1:
        return failures
    app = applications[0]
    require(app["attrs"].get("android:enabled", 0xffffffff) == 0xffffffff,
            "Application must be enabled")
    require(app["attrs"].get("android:allowBackup") == 0, "Application backup must be disabled")
    controller_metadata = [n for n in app["children"] if n["tag"] == "meta-data"
                           and n["attrs"].get("android:name") == "octosense.native.controllers"]
    require(len(controller_metadata) == 1
            and controller_metadata[0]["attrs"].get("android:value") == 0xffffffff,
            "Native Home controller endpoint must be enabled")
    sdk = [n for n in manifest["children"] if n["tag"] == "uses-sdk"]
    require(len(sdk) == 1 and sdk[0]["attrs"].get("android:targetSdkVersion") == 35,
            "Candidate must target the API 35 phone")
    require(len(sdk) == 1 and isinstance(sdk[0]["attrs"].get("android:minSdkVersion"), int)
            and sdk[0]["attrs"]["android:minSdkVersion"] <= 35, "Candidate cannot run on API 35")
    permissions = [n for n in manifest["children"] if n["tag"] == "permission"
                   and n["attrs"].get("android:name") == BIND_PERMISSION]
    require(len(permissions) == 1 and permissions[0]["attrs"].get("android:protectionLevel") == 2,
            "Home integration must require the signature-only permission")

    def component(tag, name):
        matches = [n for n in app["children"] if n["tag"] == tag and n["attrs"].get("android:name") == name]
        require(len(matches) == 1, "Missing or duplicate component: " + name)
        if len(matches) == 1:
            require(matches[0]["attrs"].get("android:enabled", 0xffffffff) == 0xffffffff,
                    "Component must be enabled: " + name)
        return matches[0] if len(matches) == 1 else {"attrs": {}, "children": []}

    home = component("service", PACKAGE + ".HomeIntegrationService")
    require(home["attrs"].get("android:permission") == BIND_PERMISSION
            and home["attrs"].get("android:exported") == 0xffffffff,
            "Home layout service boundary is incorrect")
    native = component("service", "com.android.quickstep.TouchInteractionService")
    require(native["attrs"].get("android:permission") == "android.permission.STATUS_BAR_SERVICE"
            and native["attrs"].get("android:exported") == 0xffffffff
            and native["attrs"].get("android:directBootAware") == 0xffffffff,
            "Native controller service boundary is incorrect")
    require(any(n["tag"] == "action" and n["attrs"].get("android:name") == "android.intent.action.QUICKSTEP_SERVICE"
                for n in descendants(native)), "Native Quickstep service action is missing")
    component("activity", "com.android.quickstep.RecentsActivity")
    require(not any(n["tag"] == "instrumentation" for n in manifest["children"]),
            "Validation instrumentation remains registered")
    for node in descendants(app):
        name = node["attrs"].get("android:name", "")
        if node["tag"] == "category":
            require(name not in {"android.intent.category.HOME", "android.intent.category.SECONDARY_HOME"},
                    "The separate Recents APK must not compete for HOME")
        if node["tag"] == "action":
            require(name not in {"android.content.pm.action.CONFIRM_PIN_SHORTCUT", "android.content.pm.action.CONFIRM_PIN_APPWIDGET"},
                    "Pin confirmation belongs to the Home APK")
        if node["tag"] == "provider":
            authorities = node["attrs"].get("android:authorities", "").split(";")
            require(all(a == PACKAGE or a.startswith(PACKAGE + ".") for a in authorities),
                    "Provider authority does not belong to OctoSense: " + str(authorities))
        require(name != "com.android.launcher3.notification.NotificationListener",
                "Notification listener duplicates System Bridge")

    certificates = re.findall(r"Signer #\d+ certificate SHA-256 digest: ([0-9a-f]{64})", signer_output)
    require(certificates == [expected_certificate], "APK signer does not match the existing OctoSense certificate")
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apk", type=Path, required=True)
    parser.add_argument("--aapt", required=True)
    parser.add_argument("--apksigner", required=True)
    parser.add_argument("--expected-certificate", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{64}", args.expected_certificate):
        parser.error("Expected certificate must be a lowercase SHA-256 digest")
    manifest = subprocess.check_output([args.aapt, "dump", "xmltree", str(args.apk), "AndroidManifest.xml"], text=True)
    signer = subprocess.check_output([args.apksigner, "verify", "--verbose", "--print-certs", str(args.apk)], text=True)
    failures = inspect(manifest_tree(manifest), signer, args.expected_certificate)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.with_suffix(".manifest.txt").write_text(manifest)
    args.output.with_suffix(".signer.txt").write_text(signer)
    report = {"result": "fail" if failures else "pass", "failures": failures,
              "apk_sha256": hashlib.sha256(args.apk.read_bytes()).hexdigest(),
              "expected_certificate_sha256": args.expected_certificate,
              "scope": "APK packaging and signer only", "phone_validated": False}
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    raise SystemExit(1 if failures else 0)


if __name__ == "__main__":
    main()
