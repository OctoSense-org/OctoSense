"""Actual finite system-language backend and foreground client, with deterministic native state."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from test_display_settings_backend import STUBS

ROOT = Path(__file__).resolve().parents[1]


class SystemLanguageSettingsTest(unittest.TestCase):
    def test_ordered_native_catalog_and_one_use_apply(self):
        jdk = os.environ.get('JAVA_HOME')
        javac = str(Path(jdk) / 'bin/javac') if jdk else shutil.which('javac')
        java = str(Path(jdk) / 'bin/java') if jdk else shutil.which('java')
        if not javac or not java:
            self.skipTest('JDK required')
        with tempfile.TemporaryDirectory() as tmp:
            sources = []
            for name, source in STUBS.items():
                if not (name.startswith('org/json/') or name == 'android/os/SystemClock.java'):
                    continue
                if name.endswith('JSONObject.java'):
                    source = source.replace('private final java.util.Map', 'public static final Object NULL=new Object();public long getLong(String k){return ((Number)values.get(k)).longValue();}private final java.util.Map')
                path = Path(tmp) / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(source)
                sources.append(path)
            sources += list((ROOT / 'vendor/octosense/settings/src/dev/makepad/octosense/systemlanguage').glob('*.java'))
            sources += [ROOT / 'home/resources/android/java/dev/makepad/octosense/SystemLanguageSettingsClient.java', ROOT / 'tests/java/SystemLanguageSettingsTest.java']
            for command in ([javac, '-d', tmp, *map(str, sources)], [java, '-cp', tmp, 'dev.makepad.octosense.SystemLanguageSettingsTest']):
                result = subprocess.run(command, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
