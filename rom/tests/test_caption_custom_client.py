"""Home caption process/visit authority, late replies and nonblocking retirement."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from test_display_settings_backend import STUBS
ROOT=Path(__file__).resolve().parents[1]
class CaptionCustomClientTest(unittest.TestCase):
    def test_observed_choices_and_lifecycle(self):
        jdk=os.environ.get('JAVA_HOME');javac=str(Path(jdk)/'bin/javac') if jdk else shutil.which('javac');java=str(Path(jdk)/'bin/java') if jdk else shutil.which('java')
        if not javac or not java:self.skipTest('JDK required')
        stubs={k:v for k,v in STUBS.items() if k.startswith('org/json/')}
        stubs['org/json/JSONObject.java']=stubs['org/json/JSONObject.java'].replace('public JSONObject put','public static final Object NULL=new Object();public boolean isNull(String k){return values.get(k)==null||values.get(k)==NULL;}public long getLong(String k){return ((Number)values.get(k)).longValue();}public JSONObject put')
        stubs.update({'android/os/Binder.java':'package android.os; public class Binder {}','android/os/SystemClock.java':'package android.os;public class SystemClock{public static long now;public static long elapsedRealtime(){return now;}}',
        'dev/makepad/octosense/agent/AgentPlatformClient.java':'''package dev.makepad.octosense.agent;
import org.json.JSONObject;import android.os.Binder;import java.util.concurrent.CountDownLatch;
public class AgentPlatformClient{public JSONObject state;public int writes,reads;public volatile CountDownLatch entered,proceed,closeEntered,closeProceed;public JSONObject captionCustomSnapshot(long id,Binder token,String session,long visit)throws Exception{reads++;if(entered!=null){entered.countDown();proceed.await();}return state;}public String captionCustomSet(Binder token,String session,long visit,String field,String value){writes++;return "control_applied";}public Runnable captionCustomCloseTask(Binder token,String session,long visit){return ()->{if(closeEntered!=null){closeEntered.countDown();try{closeProceed.await();}catch(InterruptedException e){Thread.currentThread().interrupt();}}};}}
'''})
        with tempfile.TemporaryDirectory() as tmp:
            sources=[]
            for name,source in stubs.items():
                path=Path(tmp)/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(source);sources.append(path)
            sources += [ROOT/'vendor/octosense/settings/src/dev/makepad/octosense/controls/CaptionCustomSettings.java',ROOT/'vendor/octosense/settings/src/dev/makepad/octosense/controls/CaptionCustomContract.java',ROOT/'home/resources/android/java/dev/makepad/octosense/CaptionCustomSettingsClient.java',ROOT/'tests/java/CaptionCustomClientTest.java']
            for cmd in ([javac,'-d',tmp,*map(str,sources)],[java,'-cp',tmp,'dev.makepad.octosense.CaptionCustomClientTest']):
                result=subprocess.run(cmd,capture_output=True,text=True,timeout=30);self.assertEqual(result.returncode,0,result.stdout+result.stderr)
