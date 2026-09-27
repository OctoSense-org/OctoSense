"""The ordinary Home client never turns stale/invalid caption rows into native writes."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from test_display_settings_backend import STUBS

ROOT=Path(__file__).resolve().parents[1]

class CaptionLanguageClientTest(unittest.TestCase):
    def test_foreground_and_observed_choice_boundary(self):
        jdk=os.environ.get('JAVA_HOME')
        javac=str(Path(jdk)/'bin/javac') if jdk else shutil.which('javac')
        java=str(Path(jdk)/'bin/java') if jdk else shutil.which('java')
        if not javac or not java:self.skipTest('JDK required')
        with tempfile.TemporaryDirectory() as tmp:
            sources=[]
            stubs={name:source for name,source in STUBS.items() if name.startswith('org/json/')}
            stubs['android/os/SystemClock.java']='package android.os; public final class SystemClock { public static long now; public static long elapsedRealtime(){return now;} }'
            for name,source in stubs.items():
                if name.endswith('JSONObject.java'):
                    source=source.replace('public int getInt(String k)', 'public long getLong(String k){return ((Number)values.get(k)).longValue();}public int getInt(String k)')
                path=Path(tmp)/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(source);sources.append(path)
            sources += [ROOT/'home/resources/android/java/dev/makepad/octosense/CaptionLanguageSettingsClient.java',ROOT/'tests/java/CaptionLanguageClientTest.java']
            for command in ([javac,'-d',tmp,*map(str,sources)],[java,'-cp',tmp,'dev.makepad.octosense.CaptionLanguageClientTest']):
                result=subprocess.run(command,capture_output=True,text=True)
                self.assertEqual(result.returncode,0,result.stdout+result.stderr)
