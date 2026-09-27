#!/usr/bin/env python3
"""Test the installed Makepad InputConnection on a dedicated API35 emulator.

Uses the actual connection source with a logging transport and View fixture.
It never reads Home data, changes device settings, or logs user-entered text.
"""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile
import time
import zipfile

PACKAGE = "dev.makepad.android.imeconnectionprobe"

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("serial", "adb", "sdk", "java-home"):
        parser.add_argument("--" + name, required=True, type=str if name == "serial" else Path)
    args = parser.parse_args()
    env = dict(os.environ, JAVA_HOME=str(args.java_home))
    def run(*command, check=True):
        return subprocess.run([str(p) for p in command], env=env, check=check, text=True, capture_output=True)
    def adb(*command, check=True):
        return run(args.adb, "-s", args.serial, *command, check=check)
    if not args.serial.startswith("emulator-") or adb("shell", "getprop", "ro.kernel.qemu").stdout.strip() != "1":
        parser.error("Use a dedicated emulator; physical devices are excluded")
    if int(adb("shell", "getprop", "ro.build.version.sdk").stdout.strip()) < 35:
        parser.error("This probe exercises API35 InputConnection query methods")
    if adb("shell", "pm", "path", PACKAGE, check=False).stdout.strip():
        parser.error("The disposable package already exists; refusing to replace it")
    root = Path(__file__).resolve().parents[3]
    runtime = root / ".sources/makepad/tools/cargo_makepad/src/android/java/dev/makepad/android"
    source = runtime / "MakepadInputConnection.java"
    fixture = Path(__file__).resolve().parents[1] / "validation-fixtures/ime-causal"
    bt = args.sdk / "build-tools/35.0.0"
    android = args.sdk / "platforms/android-35/android.jar"
    with tempfile.TemporaryDirectory(prefix="octosense-editor-probe-") as temporary:
        work = Path(temporary)
        (work / "classes").mkdir(); (work / "dex").mkdir()
        activity = (runtime / "MakepadActivity.java").read_text()
        def method(signature):
            start = activity.index(signature)
            opening = activity.index("{", start); end = opening+1; depth=1
            while depth:
                depth += (activity[end]=="{")-(activity[end]=="}"); end+=1
            return activity[start:end]
        surface = work / "MakepadSurface.java"
        surface.write_text((fixture / "MakepadSurface.java").read_text()
            .replace("/* REAL_SURFACE_UPDATE */", method("public void updateImeTextState("))
            .replace("/* REAL_SURFACE_RETIRE */", method("void retireInputConnection(")))
        run(args.java_home / "bin/javac", "-cp", android, "-d", work / "classes", source, surface,
            *sorted(path for path in fixture.glob("*.java") if path.name!="MakepadSurface.java"))
        run(args.java_home / "bin/jar", "cf", work / "classes.jar", "-C", work / "classes", ".")
        run(bt / "d8", "--lib", android, "--min-api", "26", "--output", work / "dex", work / "classes.jar")
        manifest = work / "AndroidManifest.xml"
        manifest.write_text('''<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="dev.makepad.android.imeconnectionprobe" android:versionCode="1" android:versionName="1">
          <uses-sdk android:minSdkVersion="26" android:targetSdkVersion="35"/>
          <application android:label="Disposable editor test" android:theme="@android:style/Theme.Material.Light.NoActionBar">
            <activity android:name="dev.makepad.android.ImeActivity" android:exported="true"/>
          </application></manifest>''')
        run(bt / "aapt2", "link", "--manifest", manifest, "-I", android, "-o", work / "unsigned.apk")
        with zipfile.ZipFile(work / "unsigned.apk", "a") as archive:
            archive.write(work / "dex/classes.dex", "classes.dex")
        run(bt / "zipalign", "-f", "4", work / "unsigned.apk", work / "aligned.apk")
        run(args.java_home / "bin/keytool", "-genkeypair", "-keystore", work / "ordinary.p12", "-storepass", "fixture-only", "-alias", "fixture", "-keyalg", "RSA", "-validity", "1", "-dname", "CN=Disposable Editor Fixture", "-noprompt")
        apk = work / "probe.apk"
        run(bt / "apksigner", "sign", "--ks", work / "ordinary.p12", "--ks-pass", "pass:fixture-only", "--out", apk, work / "aligned.apk")
        installed = False
        try:
            adb("install", "--no-incremental", apk); installed = True
            adb("shell", "am", "start", "-W", "-n", PACKAGE + "/dev.makepad.android.ImeActivity")
            pid = adb("shell", "pidof", PACKAGE).stdout.strip()
            if not pid.isdigit(): raise RuntimeError("Fixture process missing")
            deadline = time.monotonic() + 12
            while True:
                log = adb("logcat", "-d", "--pid=" + pid, "-s", "OctoSenseImeConnection:I", "*:S").stdout
                if "FAIL " in log: raise RuntimeError(log)
                if "PASS composition/" in log:
                    print("InputConnection cold duplicate-handle ownership, inactive/pause retirement, composition, replacement, deletion, batching and retired-query isolation passed")
                    break
                if time.monotonic() >= deadline: raise RuntimeError("No complete fixture result")
                time.sleep(.25)
        finally:
            if installed: adb("uninstall", PACKAGE)

if __name__ == "__main__": main()
