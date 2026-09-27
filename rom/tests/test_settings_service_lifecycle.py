"""Run the actual broker connection and touch guard against a deterministic looper."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
STUBS = {
    'android/os/SystemClock.java': 'package android.os; public class SystemClock {public static long now; public static long elapsedRealtime(){return now;}}',
    'android/os/Looper.java': 'package android.os; public class Looper {public static Looper getMainLooper(){return new Looper();}}',
    'android/os/IInterface.java': 'package android.os; public interface IInterface {}',
    'android/os/IBinder.java': 'package android.os; public interface IBinder {}',
    'android/os/Handler.java': '''package android.os;
        public class Handler {
            static class Task {Runnable r;long at;Task(Runnable r,long at){this.r=r;this.at=at;}}
            static java.util.List<Task> tasks=new java.util.ArrayList<>();
            public Handler(Looper l){}
            public boolean post(Runnable r){return postDelayed(r,0);}
            public boolean postDelayed(Runnable r,long delay){tasks.add(new Task(r,SystemClock.now+delay));return true;}
            public void removeCallbacks(Runnable r){tasks.removeIf(t->t.r==r);}
            public static void advance(long delta){SystemClock.now+=delta;drain();}
            public static void drain(){while(true){Task next=tasks.stream().filter(t->t.at<=SystemClock.now).findFirst().orElse(null);if(next==null)return;tasks.remove(next);next.r.run();}}
        }''',
    'android/content/ComponentName.java': 'package android.content; public class ComponentName {String pkg; public ComponentName(String p,String c){pkg=p;} public String getPackageName(){return pkg;}}',
    'android/content/Intent.java': 'package android.content; public class Intent {public Intent setComponent(ComponentName n){return this;}}',
    'android/content/ServiceConnection.java': '''package android.content;
        public interface ServiceConnection {
            void onServiceConnected(ComponentName n,android.os.IBinder b);
            void onServiceDisconnected(ComponentName n);
            void onBindingDied(ComponentName n);
            void onNullBinding(ComponentName n);
        }''',
    'android/content/pm/PackageManager.java': 'package android.content.pm; public class PackageManager {public static final int SIGNATURE_MATCH=0;public int signature;public int checkSignatures(String a,String b){return signature;}}',
    'android/content/Context.java': '''package android.content;
        public class Context {
            public static final int BIND_AUTO_CREATE=1; public int binds,unbinds;
            public boolean available=true; public ServiceConnection connection;
            public android.content.pm.PackageManager pm=new android.content.pm.PackageManager();
            public android.content.pm.PackageManager getPackageManager(){return pm;}
            public String getPackageName(){return "agent";}
            public boolean bindService(Intent i,ServiceConnection c,int flags){binds++;if(available)connection=c;return available;}
            public void unbindService(ServiceConnection c){if(connection!=c)throw new AssertionError("wrong binding");unbinds++;connection=null;}
        }''',
    'android/view/MotionEvent.java': '''package android.view; public class MotionEvent {
        public static final int ACTION_DOWN=0,FLAG_WINDOW_IS_OBSCURED=1;
        private final int action,flags; public MotionEvent(int a,int f){action=a;flags=f;}
        public int getActionMasked(){return action;} public int getFlags(){return flags;}
        }''',
}


class SettingsServiceLifecycleTest(unittest.TestCase):
    def test_lazy_binding_idle_release_death_close_and_obscured_gestures(self):
        jdk = os.environ.get('JAVA_HOME')
        javac = str(Path(jdk) / 'bin/javac') if jdk else shutil.which('javac')
        java = str(Path(jdk) / 'bin/java') if jdk else shutil.which('java')
        if not javac or not java:
            self.skipTest('JDK required')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            sources = []
            for name, source in STUBS.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(source)
                sources.append(path)
            sources += [ROOT / 'vendor/octosense/agent/src/dev/makepad/octosense/agent/SettingsServiceConnection.java',
                        ROOT / 'home/resources/android/java/dev/makepad/octosense/ObscuredTouchGuard.java',
                        ROOT / 'tests/java/SettingsServiceLifecycleTest.java']
            for command in ([javac, '-d', directory, *map(str, sources)],
                            [java, '-cp', directory, 'dev.makepad.octosense.agent.SettingsServiceLifecycleTest']):
                result = subprocess.run(command, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
