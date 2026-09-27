"""Actual Agent Binder cohort owner/death handling, with native storage isolated."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from test_display_settings_backend import STUBS
ROOT=Path(__file__).resolve().parents[1]
class CaptionCustomSessionsTest(unittest.TestCase):
    def test_live_process_owner_and_delayed_death(self):
        jdk=os.environ.get('JAVA_HOME');javac=str(Path(jdk)/'bin/javac') if jdk else shutil.which('javac');java=str(Path(jdk)/'bin/java') if jdk else shutil.which('java')
        if not javac or not java:self.skipTest('JDK required')
        stubs={k:v for k,v in STUBS.items() if k.startswith('org/json/')}
        stubs['org/json/JSONObject.java']=stubs['org/json/JSONObject.java'].replace('public JSONObject put','public static final Object NULL=new Object();public JSONObject put')
        stubs.update({'android/content/Context.java':'package android.content;public class Context{}','android/os/RemoteException.java':'package android.os;public class RemoteException extends Exception{}','android/os/IBinder.java':'package android.os;public interface IBinder{interface DeathRecipient{void binderDied();}boolean isBinderAlive();void linkToDeath(DeathRecipient d,int flags)throws RemoteException;boolean unlinkToDeath(DeathRecipient d,int flags);}',
        'dev/makepad/octosense/agent/CaptionCustomPlatformSettings.java':'''package dev.makepad.octosense.agent;import android.content.Context;import dev.makepad.octosense.controls.CaptionCustomSettings.Field;
final class CaptionCustomPlatformSettings{static int writes,closes;String session;long visit;CaptionCustomPlatformSettings(Context c){}void replaceSession(String s){session=s;visit=0;}void invalidateSession(String s){if(s.equals(session)){session=null;visit=0;closes++;}}boolean enterScope(String s,long v){if(!s.equals(session))return false;visit=v;return true;}boolean scopeActive(String s,long v){return s.equals(session)&&v==visit;}Boolean customSelected(){return true;}String read(Field f){return "observed";}String[] choices(String s,long v,Field f){return scopeActive(s,v)?f.choices():new String[0];}String apply(String s,long v,Field f,String c){if(!scopeActive(s,v))return "control_unavailable";writes++;return "control_applied";}void leaveScope(String s,long v){if(scopeActive(s,v))visit=0;}}
'''})
        with tempfile.TemporaryDirectory() as tmp:
            sources=[]
            for name,source in stubs.items():
                p=Path(tmp)/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(source);sources.append(p)
            sources += [ROOT/'vendor/octosense/settings/src/dev/makepad/octosense/controls/CaptionCustomSettings.java',ROOT/'vendor/octosense/settings/src/dev/makepad/octosense/controls/CaptionCustomContract.java',ROOT/'vendor/octosense/agent/src/dev/makepad/octosense/agent/CaptionCustomSessions.java',ROOT/'tests/java/CaptionCustomSessionsTest.java']
            for cmd in ([javac,'-d',tmp,*map(str,sources)],[java,'-cp',tmp,'dev.makepad.octosense.agent.CaptionCustomSessionsTest']):
                result=subprocess.run(cmd,capture_output=True,text=True,timeout=30);self.assertEqual(result.returncode,0,result.stdout+result.stderr)
