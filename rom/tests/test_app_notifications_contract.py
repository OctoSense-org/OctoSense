import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class AppNotificationsContractTest(unittest.TestCase):
    def test_target_leases_policies_and_linked_permission_updates(self):
        java_home = os.environ.get("JAVA_HOME")
        javac = str(Path(java_home) / "bin/javac") if java_home else shutil.which("javac")
        java = str(Path(java_home) / "bin/java") if java_home else shutil.which("java")
        if not javac or not java:
            self.skipTest("JDK required")
        with tempfile.TemporaryDirectory() as output:
            source = Path(output) / "org/json"
            source.mkdir(parents=True)
            (source / "JSONObject.java").write_text('package org.json;public final class JSONObject {public static final Object NULL=new Object();public JSONObject put(String key,Object value){return this;}}')
            (source / "JSONArray.java").write_text('package org.json;public final class JSONArray {}')
            files = [*source.glob("*.java"), ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/notifications/AppNotificationsContract.java", ROOT / "tests/java/AppNotificationsContractTest.java"]
            result = subprocess.run([javac, "-d", output, *map(str, files)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            result = subprocess.run([java, "-cp", output, "AppNotificationsContractTest"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
