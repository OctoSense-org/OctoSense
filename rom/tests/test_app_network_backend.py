"""Exercise actual UID-policy authority, coupling, and partial-write readback."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from test_display_settings_backend import STUBS
ROOT=Path(__file__).resolve().parents[1]

class AppNetworkBackendTest(unittest.TestCase):
    def test_observed_uid_policy_and_one_use_authority(self):
        jdk=os.environ.get('JAVA_HOME')
        javac=str(Path(jdk)/'bin/javac') if jdk else shutil.which('javac')
        java=str(Path(jdk)/'bin/java') if jdk else shutil.which('java')
        if not javac or not java:self.skipTest('JDK required')
        with tempfile.TemporaryDirectory() as tmp:
            sources=[]
            for name,source in STUBS.items():
                if not name.startswith('org/json/'):continue
                if name.endswith('JSONObject.java'):source=source.replace('private final java.util.Map','public static final Object NULL=new Object();private final java.util.Map')
                path=Path(tmp)/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(source);sources.append(path)
            sources+=list((ROOT/'vendor/octosense/settings/src/dev/makepad/octosense/appnetwork').glob('*.java'))
            sources.append(ROOT/'tests/java/AppNetworkBackendTest.java')
            for command in ([javac,'-d',tmp,*map(str,sources)],[java,'-cp',tmp,'AppNetworkBackendTest']):
                result=subprocess.run(command,capture_output=True,text=True)
                self.assertEqual(result.returncode,0,result.stdout+result.stderr)
