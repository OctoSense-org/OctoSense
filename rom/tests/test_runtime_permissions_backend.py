"""Exercise real native-operation authority without granting Android permissions."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from test_default_roles_backend import STUBS as ROLE_STUBS

ROOT=Path(__file__).resolve().parents[1]
NATIVE=ROOT/'home/android/platform-build/permissioncontroller/files/src/com/android/permissioncontroller/octosense'
STUBS=dict(ROLE_STUBS)
STUBS['com/android/permissioncontroller/octosense/OctoSensePermissionOperationActivity.java']='package com.android.permissioncontroller.octosense; public final class OctoSensePermissionOperationActivity {}'
STUBS['com/android/permissioncontroller/octosense/NativePermissionModel.java']='''package com.android.permissioncontroller.octosense;
import dev.makepad.octosense.permissions.PermissionsSettingsContract.*;
public final class NativePermissionModel {
    public static State next;public static int reads;NativePermissionModel(android.content.Context c){}
    State read(String p,Group g){reads++;return next;}
    static final class Row {Group group;String name,label,category,subtitle,target;}
    static final class Option {final Choice choice;final String label;final boolean selected,enabled;
        Option(Choice c,boolean s,boolean e){choice=c;label=c.label;selected=s;enabled=e&&c.actionable()&&!s;}}
    static final class State {String pkg,identity,fingerprint,generation,label,detail;Group group;boolean exists,groupAvailable;
        final java.util.List<Row> rows=new java.util.ArrayList<>();final java.util.List<Option> choices=new java.util.ArrayList<>();
        Option option(Choice c){for(Option o:choices)if(o.choice==c)return o;return null;}}
}'''

class RuntimePermissionsBackendTest(unittest.TestCase):
    def test_observed_choices_native_tickets_warning_recheck_and_readback(self):
        jdk=os.environ.get('JAVA_HOME');javac=str(Path(jdk)/'bin/javac') if jdk else shutil.which('javac');java=str(Path(jdk)/'bin/java') if jdk else shutil.which('java')
        if not javac or not java:self.skipTest('JDK required')
        with tempfile.TemporaryDirectory() as directory:
            out=Path(directory);sources=[]
            for name,body in STUBS.items():
                source=out/name;source.parent.mkdir(parents=True,exist_ok=True);source.write_text(body);sources.append(source)
            sources.extend([ROOT/'vendor/octosense/settings/src/dev/makepad/octosense/permissions/PermissionsSettingsContract.java',NATIVE/'PermissionSettingsBackend.java',ROOT/'tests/java/RuntimePermissionsBackendTest.java'])
            result=subprocess.run([javac,'-d',str(out),*map(str,sources)],capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stdout+result.stderr)
            result=subprocess.run([java,'-cp',str(out),'com.android.permissioncontroller.octosense.RuntimePermissionsBackendTest'],capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stdout+result.stderr)

if __name__=='__main__':unittest.main()
