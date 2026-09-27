"""Execute the real shared backend with deterministic platform/time/lock services."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
ROOT=Path(__file__).resolve().parents[1]
STUBS={
'android/os/SystemClock.java':'package android.os; public final class SystemClock {public static long now=100; public static long elapsedRealtime(){return now;}}',
'android/os/Process.java':'package android.os; public final class Process {public static int myUid(){return 10123;}}',
'android/os/UserManager.java':'package android.os; public final class UserManager {public boolean unlocked=true;public boolean isUserUnlocked(){return unlocked;}}',
'android/app/KeyguardManager.java':'package android.app; public final class KeyguardManager {public boolean locked;public boolean isKeyguardLocked(){return locked;}}',
'android/content/Context.java':'''package android.content; public final class Context {public android.os.UserManager users=new android.os.UserManager();public android.app.KeyguardManager lock=new android.app.KeyguardManager();public <T>T getSystemService(Class<T> type){return type.cast(type==android.os.UserManager.class?users:lock);}public Resources getResources(){return new Resources();}public static class Resources {public Metrics getDisplayMetrics(){return new Metrics();}}public static class Metrics {public int densityDpi=420;}}''',
'org/json/JSONObject.java':'''package org.json;public final class JSONObject {private final java.util.Map<String,Object> values=new java.util.LinkedHashMap<>();public JSONObject put(String k,Object v){values.put(k,v);return this;}public Object opt(String k){return values.get(k);}public JSONObject getJSONObject(String k){return (JSONObject)values.get(k);}public JSONArray getJSONArray(String k){return (JSONArray)values.get(k);}public String getString(String k){return (String)values.get(k);}public int getInt(String k){return ((Number)values.get(k)).intValue();}public boolean getBoolean(String k){return (Boolean)values.get(k);}public int optInt(String k,int fallback){return values.containsKey(k)?getInt(k):fallback;}public boolean optBoolean(String k,boolean fallback){return values.containsKey(k)?getBoolean(k):fallback;}public String toString(){return values.toString();}}''',
'org/json/JSONArray.java':'''package org.json;public final class JSONArray{private final java.util.List<Object> values=new java.util.ArrayList<>();public JSONArray put(Object v){values.add(v);return this;}public int length(){return values.size();}public JSONObject getJSONObject(int i){return (JSONObject)values.get(i);}public String getString(int i){return (String)values.get(i);}public String toString(){return values.toString();}}''',
}
class DisplaySettingsBackendTest(unittest.TestCase):
 def test_fresh_observations_policy_and_actual_readback(self):
  jdk=os.environ.get('JAVA_HOME');javac=str(Path(jdk)/'bin/javac') if jdk else shutil.which('javac');java=str(Path(jdk)/'bin/java') if jdk else shutil.which('java')
  if not javac or not java:self.skipTest('JDK required')
  with tempfile.TemporaryDirectory() as tmp:
   files=[]
   for name,source in STUBS.items():
    p=Path(tmp)/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(source);files.append(p)
   folder=ROOT/'vendor/octosense/settings/src/dev/makepad/octosense/display'
   files += [folder/'DisplaySettingsContract.java',folder/'DisplaySettingsBackend.java',ROOT/'tests/java/DisplaySettingsBackendTest.java']
   result=subprocess.run([javac,'-d',tmp,*map(str,files)],capture_output=True,text=True);self.assertEqual(result.returncode,0,result.stdout+result.stderr)
   result=subprocess.run([java,'-cp',tmp,'DisplaySettingsBackendTest'],capture_output=True,text=True);self.assertEqual(result.returncode,0,result.stdout+result.stderr)
