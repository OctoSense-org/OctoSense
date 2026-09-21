#!/usr/bin/env python3
"""Prepare the reviewed OnePlus 6 module; never install it or change the phone."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import zipfile


PACKAGE = "dev.makepad.octosense.quickstep"
MODULE = "octosense_quickstep_enchilada"
FRAMEWORK_SHA = "f23f0e7d35876801cb4eb4bf318496db10561c73ca51af408ce066c74ea34b03"
PRIVILEGED = {
    "ACCESS_CONTEXTUAL_SEARCH", "ACCESS_HIDDEN_PROFILES_FULL", "ALLOW_SLIPPERY_TOUCHES",
    "BIND_APPWIDGET", "BROADCAST_CLOSE_SYSTEM_DIALOGS",
    "CONTROL_REMOTE_APP_TRANSITION_ANIMATIONS", "INTERACT_ACROSS_USERS",
    "START_TASKS_FROM_RECENTS", "STATUS_BAR", "STOP_APP_SWITCHES", "WRITE_SECURE_SETTINGS",
}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["apk", "inspection", "permission-levels", "framework-res", "build-tools",
                 "keystore", "output"]:
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--key-alias", required=True)
    parser.add_argument("--store-password-env", required=True)
    parser.add_argument("--key-password-env", required=True)
    parser.add_argument("--upgrade-from-apk", type=Path,
                        help="Exact currently mounted native APK permitted as an upgrade baseline")
    args = parser.parse_args()
    inspection = json.loads(args.inspection.read_text())
    if inspection.get("result") != "pass" or inspection.get("apk_sha256") != sha(args.apk):
        raise RuntimeError("A passing inspection of this exact signed native APK is required")
    certificate = inspection["expected_certificate_sha256"]
    if not re.fullmatch(r"[0-9a-f]{64}", certificate):
        raise RuntimeError("Invalid inspected certificate")
    if sha(args.framework_res) != FRAMEWORK_SHA:
        raise RuntimeError("Framework resources differ from the reviewed OnePlus 6 ROM")
    levels = json.loads(args.permission_levels.read_text())
    requested = set(re.findall(r"uses-permission: name='([^']+)'", subprocess.check_output(
        [str(args.build_tools / "aapt"), "dump", "permissions", str(args.apk)], text=True)))
    if requested != set(levels):
        raise RuntimeError("Permission inventory differs from the actual APK")
    privileged = {name for name, level in levels.items() if "privileged" in level.split("|")}
    if privileged != {"android.permission." + name for name in PRIVILEGED}:
        raise RuntimeError("Privileged permission set differs from the reviewed ROM")
    args.output.mkdir(parents=True, exist_ok=True)
    if any(args.output.iterdir()):
        raise RuntimeError("Use an empty output directory to preserve earlier module evidence")
    aapt = str(args.build_tools / "aapt2")
    signer = str(args.build_tools / "apksigner")
    actual_signer = subprocess.check_output([signer, "verify", "--print-certs", str(args.apk)], text=True)
    if re.findall(r"Signer #\d+ certificate SHA-256 digest: ([0-9a-f]{64})", actual_signer) != [certificate]:
        raise RuntimeError("Native APK signer differs from its inspection")
    overlayable = subprocess.check_output([aapt, "dump", "overlayable", str(args.framework_res)], text=True)
    if overlayable.strip():
        raise RuntimeError("Framework now defines overlayable resources; review its policy first")
    with tempfile.TemporaryDirectory(prefix="octosense-module-") as temporary:
        work = Path(temporary)
        res = work / "res"
        (res / "values").mkdir(parents=True)
        (res / "xml").mkdir()
        (res / "values/config.xml").write_text('<resources><string name="octosense_recents" translatable="false">'
            + PACKAGE + '/com.android.quickstep.RecentsActivity</string></resources>\n')
        (res / "xml/overlays.xml").write_text('<overlay><item target="string/config_recentsComponentName" '
            'value="@string/octosense_recents" /></overlay>\n')
        manifest = work / "AndroidManifest.xml"
        manifest.write_text('<manifest xmlns:android="http://schemas.android.com/apk/res/android" '
            'package="' + PACKAGE + '.overlay" android:versionCode="1" android:versionName="1.0.0">'
            '<application android:hasCode="false" android:allowBackup="false" />'
            '<overlay android:targetPackage="android" android:isStatic="true" android:priority="999" '
            'android:resourcesMap="@xml/overlays" /></manifest>\n')
        subprocess.run([aapt, "compile", "--dir", str(res), "-o", str(work / "resources.zip")], check=True)
        unsigned = work / "overlay-unsigned.apk"
        subprocess.run([aapt, "link", "-I", str(args.framework_res), "--manifest", str(manifest),
            "--min-sdk-version", "35", "--target-sdk-version", "35", "--no-resource-deduping",
            "--no-resource-removal", "-o", str(unsigned), str(work / "resources.zip")], check=True)
        module = work / MODULE
        extension = module / "system/system_ext"
        native = extension / "priv-app/OctoSenseQuickstep/OctoSenseQuickstep.apk"
        native.parent.mkdir(parents=True)
        shutil.copy2(args.apk, native)
        overlay = extension / "overlay/OctoSenseRecentsOverlay/OctoSenseRecentsOverlay.apk"
        overlay.parent.mkdir(parents=True)
        subprocess.run([signer, "sign", "--ks", str(args.keystore), "--ks-key-alias", args.key_alias,
            "--ks-pass", "env:" + args.store_password_env, "--key-pass", "env:" + args.key_password_env,
            "--out", str(overlay), str(unsigned)], check=True)
        overlay_signer = subprocess.check_output([signer, "verify", "--print-certs", str(overlay)], text=True)
        if re.findall(r"Signer #\d+ certificate SHA-256 digest: ([0-9a-f]{64})", overlay_signer) != [certificate]:
            raise RuntimeError("Overlay signer differs from native APK")
        xml = extension / "etc/permissions/privapp-permissions-octosense-quickstep.xml"
        xml.parent.mkdir(parents=True)
        xml.write_text('<permissions>\n  <privapp-permissions package="' + PACKAGE + '">\n'
            + ''.join('    <permission name="' + p + '" />\n' for p in sorted(privileged))
            + '  </privapp-permissions>\n</permissions>\n')
        (module / "module.prop").write_text('id=' + MODULE + '\nname=OctoSense Quickstep (OnePlus 6 experiment)\n'
            'version=1.0.0\nversionCode=1\nauthor=OctoSense\n'
            'description=ROM-specific native Recents prototype; requires separately approved deployment.\n')
        customize = Path(__file__).with_name("quickstep-module-customize.sh").read_text()
        if args.upgrade_from_apk:
            customize = 'OCTOSENSE_UPGRADE_FROM_SHA=' + sha(args.upgrade_from_apk) + '\n' + customize
        (module / "customize.sh").write_text(customize)
        files = {str(p.relative_to(module)): sha(p) for p in sorted(module.rglob("*")) if p.is_file()}
        (module / "payload.sha256").write_text(''.join(h + '  ' + p + '\n' for p, h in files.items()))
        package = args.output / (MODULE + ".zip")
        with zipfile.ZipFile(package, "w", zipfile.ZIP_DEFLATED) as archive:
            for p in sorted(module.rglob("*")):
                if p.is_file():
                    archive.write(p, p.relative_to(module))
        # Keep the exact reviewable payload and overlay source beside the archive.
        destination = args.output / MODULE
        shutil.copytree(module, destination)
        shutil.copytree(res, args.output / "overlay-source/res")
        shutil.copy2(manifest, args.output / "overlay-source/AndroidManifest.xml")
        report = {"status": "packaged", "module_sha256": sha(package), "payload": files,
                  "certificate_sha256": certificate, "framework_res_sha256": FRAMEWORK_SHA,
                  "permission_inventory_sha256": sha(args.permission_levels),
                  "privileged_permissions": sorted(privileged), "phone_deployed": False,
                  "runtime_grants_verified": False, "rollback_tested": False}
        report["upgrade_from_apk_sha256"] = sha(args.upgrade_from_apk) if args.upgrade_from_apk else None
        (args.output / "module-result.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
