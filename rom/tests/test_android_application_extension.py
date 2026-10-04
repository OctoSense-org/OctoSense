"""Run the pinned extension loader with JVM doubles for Android metadata."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET

from test_android_ime_handoff import method

ROOT = Path(__file__).resolve().parents[2]
ANDROID = "{http://schemas.android.com/apk/res/android}"


class ApplicationExtensionTests(unittest.TestCase):
    def test_manifest_extension_survives_a_test_package_name(self):
        runtime = ROOT / ".sources/makepad/tools/cargo_makepad/src/android/java/dev/makepad/android/MakepadActivity.java"
        if not runtime.exists():
            self.skipTest("run tools/setup.py to prepare the pinned runtime")
        jdk = os.environ.get("JAVA_HOME")
        javac = str(Path(jdk) / "bin/javac") if jdk else shutil.which("javac")
        java = str(Path(jdk) / "bin/java") if jdk else shutil.which("java")
        if not javac or not java:
            self.skipTest("JDK required")
        manifest = ET.parse(ROOT / "phone/resources/android/AndroidManifest.xml.template")
        entry = next(node for node in manifest.findall("application/meta-data")
                     if node.get(ANDROID + "name") == "dev.makepad.android.APPLICATION_EXTENSION")
        configured = entry.get(ANDROID + "value")
        # Verify the product opts into the same contract exercised below.
        self.assertTrue((ROOT / "phone/resources/android/java" / (configured.replace(".", "/") + ".java")).is_file())
        harness = (ROOT / "rom/tests/java/ApplicationExtensionTest.java").read_text()
        harness = harness.replace("/* LOADER */", method(runtime.read_text(), "private void createApplicationExtension()"))
        sources = {
            "dev/makepad/android/ApplicationExtensionTest.java": harness,
            "android/os/Bundle.java": """package android.os;
                public class Bundle extends java.util.HashMap<String,String> {
                    public String getString(String key) { return get(key); }
                }""",
            "android/content/pm/ApplicationInfo.java": """package android.content.pm;
                public class ApplicationInfo { public android.os.Bundle metaData; }""",
            "android/content/pm/PackageManager.java": """package android.content.pm;
                public class PackageManager {
                    public static final int GET_META_DATA = 128;
                    public ApplicationInfo info = new ApplicationInfo();
                    public ApplicationInfo getApplicationInfo(String name, int flags) throws NameNotFoundException {
                        if (flags != GET_META_DATA) throw new AssertionError("metadata flag missing");
                        return info;
                    }
                    public static class NameNotFoundException extends Exception {}
                }""",
        }
        with tempfile.TemporaryDirectory() as temp:
            paths = []
            for name, source in sources.items():
                path = Path(temp) / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(source)
                paths.append(str(path))
            result = subprocess.run([javac, "-d", temp, *paths], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            result = subprocess.run([java, "-cp", temp, "dev.makepad.android.ApplicationExtensionTest"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
