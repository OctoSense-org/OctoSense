"""Finite keyboard authority and actual Home visible-page client; no Android writes."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from test_display_settings_backend import STUBS

ROOT = Path(__file__).resolve().parents[1]

class KeyboardSettingsTest(unittest.TestCase):
    def test_native_authority_and_foreground_flow(self):
        jdk = os.environ.get('JAVA_HOME')
        javac = str(Path(jdk)/'bin/javac') if jdk else shutil.which('javac')
        java = str(Path(jdk)/'bin/java') if jdk else shutil.which('java')
        if not javac or not java:
            self.skipTest('JDK required')
        with tempfile.TemporaryDirectory() as tmp:
            sources=[]
            stubs={name:source for name,source in STUBS.items() if name.startswith('org/json/') or name=='android/os/SystemClock.java'}
            stubs['org/json/JSONObject.java']=stubs['org/json/JSONObject.java'].replace('private final java.util.Map','public static final Object NULL=new Object();public long getLong(String k){return ((Number)values.get(k)).longValue();}private final java.util.Map')
            stubs['android/app/PendingIntent.java']='package android.app;public class PendingIntent{}'
            stubs['org/json/JSONException.java']='package org.json;public class JSONException extends RuntimeException{}'
            for name,source in stubs.items():
                path=Path(tmp)/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(source);sources.append(path)
            sources+=list((ROOT/'vendor/octosense/settings/src/dev/makepad/octosense/keyboards').glob('*.java'))
            sources+=[ROOT/'home/resources/android/java/dev/makepad/octosense/KeyboardSettingsClient.java',ROOT/'tests/java/KeyboardPolicyTest.java',ROOT/'tests/java/KeyboardSettingsClientTest.java']
            commands=[[javac,'-d',tmp,*map(str,sources)],[java,'-cp',tmp,'dev.makepad.octosense.keyboards.KeyboardPolicyTest'],[java,'-cp',tmp,'dev.makepad.octosense.KeyboardSettingsClientTest']]
            for command in commands:
                result=subprocess.run(command,capture_output=True,text=True)
                self.assertEqual(result.returncode,0,result.stdout+result.stderr)
