"""Native hearing preference semantics and finite mutation choices without an Android device."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
ROOT=Path(__file__).resolve().parents[1]
class HearingSettingsTest(unittest.TestCase):
    def test_raw_service_readback_and_finite_hearing_choices(self):
        jdk=os.environ.get('JAVA_HOME')
        javac=str(Path(jdk)/'bin/javac') if jdk else shutil.which('javac')
        java=str(Path(jdk)/'bin/java') if jdk else shutil.which('java')
        if not javac or not java:self.skipTest('JDK required')
        with tempfile.TemporaryDirectory() as tmp:
            sources=[ROOT/'vendor/octosense/settings/src/dev/makepad/octosense/controls/HearingSettings.java',ROOT/'tests/java/HearingSettingsTest.java']
            for command in ([javac,'-d',tmp,*map(str,sources)],[java,'-cp',tmp,'HearingSettingsTest']):
                result=subprocess.run(command,capture_output=True,text=True)
                self.assertEqual(result.returncode,0,result.stdout+result.stderr)
