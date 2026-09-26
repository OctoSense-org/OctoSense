"""Execute the actual policy copy/lease backend with deterministic native observations."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from test_display_settings_backend import STUBS
ROOT=Path(__file__).resolve().parents[1]

class DndSettingsBackendTest(unittest.TestCase):
    def test_verified_schedule_owner_aliases_do_not_admit_foreign_providers(self):
        jdk=os.environ.get('JAVA_HOME');javac=str(Path(jdk)/'bin/javac') if jdk else shutil.which('javac');java=str(Path(jdk)/'bin/java') if jdk else shutil.which('java')
        if not javac or not java:self.skipTest('JDK required')
        source=ROOT/'vendor/octosense/settings-broker/src/dev/makepad/octosense/settingsbroker/DndScheduleOwner.java'
        with tempfile.TemporaryDirectory() as temporary:
            built=subprocess.run([javac,'-d',temporary,str(source),str(ROOT/'tests/java/DndScheduleOwnerTest.java')],capture_output=True,text=True)
            self.assertEqual(built.returncode,0,built.stdout+built.stderr)
            result=subprocess.run([java,'-cp',temporary,'dev.makepad.octosense.settingsbroker.DndScheduleOwnerTest'],capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stdout+result.stderr)

    def test_finite_policy_copy_and_observed_native_rule_authority(self):
        jdk=os.environ.get('JAVA_HOME');javac=str(Path(jdk)/'bin/javac') if jdk else shutil.which('javac');java=str(Path(jdk)/'bin/java') if jdk else shutil.which('java')
        if not javac or not java:self.skipTest('JDK required')
        with tempfile.TemporaryDirectory() as temporary:
            files=[]
            for name,source in STUBS.items():
                if not name.startswith('org/json/'):continue
                if name.endswith('JSONObject.java'):source=source.replace('private final java.util.Map','public static final Object NULL=new Object();private final java.util.Map')
                file=Path(temporary)/name;file.parent.mkdir(parents=True,exist_ok=True);file.write_text(source);files.append(file)
            files+=list((ROOT/'vendor/octosense/settings/src/dev/makepad/octosense/dnd').glob('*.java'))
            files.append(ROOT/'tests/java/DndSettingsBackendTest.java')
            built=subprocess.run([javac,'-d',temporary,*map(str,files)],capture_output=True,text=True)
            self.assertEqual(built.returncode,0,built.stdout+built.stderr)
            result=subprocess.run([java,'-cp',temporary,'DndSettingsBackendTest'],capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stdout+result.stderr)

    def test_native_activity_methods_preserve_platform_semantics_and_unknown_state(self):
        jdk=os.environ.get('JAVA_HOME');javac=str(Path(jdk)/'bin/javac') if jdk else shutil.which('javac');java=str(Path(jdk)/'bin/java') if jdk else shutil.which('java')
        if not javac or not java:self.skipTest('JDK required')
        source=ROOT/'vendor/octosense/settings-broker/src/dev/makepad/octosense/settingsbroker/DndRuleActivity.java'
        with tempfile.TemporaryDirectory() as temporary:
            built=subprocess.run([javac,'-d',temporary,str(source),str(ROOT/'tests/java/DndRuleActivityTest.java')],capture_output=True,text=True)
            self.assertEqual(built.returncode,0,built.stdout+built.stderr)
            result=subprocess.run([java,'-cp',temporary,'dev.makepad.octosense.settingsbroker.DndRuleActivityTest'],capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stdout+result.stderr)
