"""Run the real observer adapter against native-like LiveData readiness states."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from test_default_roles_backend import STUBS as ROLE_STUBS
ROOT=Path(__file__).resolve().parents[1]
STUBS={name:body for name,body in ROLE_STUBS.items() if name.startswith('org/json/') or name in ['android/os/UserHandle.java','android/os/Process.java','android/os/SystemClock.java','android/content/pm/SigningInfo.java','android/content/pm/Signature.java']}
STUBS.update({
'android/content/Context.java':'''package android.content;public class Context {
 public final android.content.pm.PackageManager packages=new android.content.pm.PackageManager();
 public Context getApplicationContext(){return this;}public android.content.pm.PackageManager getPackageManager(){return packages;}
 public <T>T getSystemService(Class<T> c){return c.cast(new android.app.AppOpsManager());}
 public android.content.res.Resources getResources(){return new android.content.res.Resources();}
 public String getPackageName(){return "controller";}public String getString(int i,Object...args){return "Native detail";}}
''',
'android/content/res/Resources.java':'package android.content.res;public class Resources{public int getIdentifier(String n,String t,String p){return 0;}}',
'android/app/Application.java':'package android.app;public class Application extends android.content.Context{}',
'android/app/AppOpsManager.java':'package android.app;public class AppOpsManager{public static String permissionToOp(String p){return null;}public int unsafeCheckOpRawNoThrow(String p,int uid,String pkg){return 1;}}',
'android/content/pm/ApplicationInfo.java':'''package android.content.pm;public class ApplicationInfo{public static final int FLAG_INSTALLED=1,FLAG_SUSPENDED=2,FLAG_SYSTEM=4,FLAG_UPDATED_SYSTEM_APP=8;public int uid=10123,flags=1;public boolean enabled=true;public CharSequence loadLabel(PackageManager pm){return "Fixture";}}''',
'android/content/pm/PackageInfo.java':'''package android.content.pm;public class PackageInfo{public ApplicationInfo applicationInfo=new ApplicationInfo();public SigningInfo signingInfo=new SigningInfo();public long firstInstallTime=1,lastUpdateTime=2;public String[] requestedPermissions={"undefined"};public long getLongVersionCode(){return 1;}}''',
'android/content/pm/PackageManager.java':'''package android.content.pm;public class PackageManager{public static final int GET_PERMISSIONS=1,GET_SIGNING_CERTIFICATES=2;public PackageInfo getPackageInfo(String p,int f)throws NameNotFoundException{return new PackageInfo();}public int checkPermission(String p,String pkg){return -1;}public int getPermissionFlags(String p,String pkg,android.os.UserHandle user){return 0;}public PermissionGroupInfo getPermissionGroupInfo(String p,int f)throws NameNotFoundException{throw new NameNotFoundException();}public static class NameNotFoundException extends Exception{}}''',
'android/content/pm/PermissionGroupInfo.java':'package android.content.pm;public class PermissionGroupInfo{public CharSequence loadLabel(PackageManager p){return "Camera";}}',
'android/os/Looper.java':'''package android.os;public class Looper{static final Looper MAIN=new Looper();public static final ThreadLocal<Boolean> IS_MAIN=ThreadLocal.withInitial(()->false);public static Looper getMainLooper(){return MAIN;}public static Looper myLooper(){return IS_MAIN.get()?MAIN:null;}}''',
'android/os/Handler.java':'''package android.os;public class Handler{public static final java.util.Queue<Runnable> queue=new java.util.concurrent.ConcurrentLinkedQueue<>();public Handler(Looper l){}public boolean post(Runnable r){queue.add(r);return true;}public static void drain(){boolean old=Looper.IS_MAIN.get();Looper.IS_MAIN.set(true);try{Runnable r;while((r=queue.poll())!=null)r.run();}finally{Looper.IS_MAIN.set(old);}}}''',
'androidx/lifecycle/Observer.java':'package androidx.lifecycle;public interface Observer<T>{void onChanged(T value);}',
'androidx/lifecycle/ViewModelStore.java':'''package androidx.lifecycle;public class ViewModelStore{public static int clearCount;public void put(String key,Object model){}public void clear(){clearCount++;}}''',
'androidx/lifecycle/TestLiveData.java':'''package androidx.lifecycle;public class TestLiveData<T>{public static int active;public T value;public boolean stale;final java.util.List<Observer<T>> observers=new java.util.ArrayList<>();public boolean isStale(){return stale;}public T getValue(){return value;}public void observeForever(Observer<T> o){observers.add(o);active++;o.onChanged(value);}public void removeObserver(Observer<T> o){if(observers.remove(o))active--;}public void emit(T v,boolean s){value=v;stale=s;for(Observer<T>o:new java.util.ArrayList<>(observers))o.onChanged(v);}}''',
'kotlin/Pair.java':'package kotlin;public class Pair<A,B>{public A getFirst(){return null;}public B getSecond(){return null;}}',
'com/android/permissioncontroller/permission/ui/Category.java':'package com.android.permissioncontroller.permission.ui;public enum Category{ALLOWED,ASK,DENIED}',
'com/android/permissioncontroller/permission/utils/v35/MultiDeviceUtils.java':'''package com.android.permissioncontroller.permission.utils.v35;public class MultiDeviceUtils{public static boolean isDefaultDeviceId(String id){return "default".equals(id);}public static String getDefaultDevicePersistentDeviceId(){return "default";}}''',
'com/android/permissioncontroller/permission/ui/model/AppPermissionGroupsViewModel.java':'''package com.android.permissioncontroller.permission.ui.model;
import java.util.*;import androidx.lifecycle.*;import com.android.permissioncontroller.permission.ui.Category;
public class AppPermissionGroupsViewModel{
 public static boolean initialStale;public static AppPermissionGroupsViewModel last;final TestLiveData<Map<Category,List<GroupUiInfo>>> live=new TestLiveData<>();
 public AppPermissionGroupsViewModel(String pkg,android.os.UserHandle user,long id){last=this;live.stale=initialStale;live.value=Collections.singletonMap(Category.DENIED,Collections.singletonList(new GroupUiInfo()));}
 public TestLiveData<Map<Category,List<GroupUiInfo>>> getPackagePermGroupsLiveData(){return live;}
 public enum Subtitle{NONE,FOREGROUND_ONLY,BACKGROUND,MEDIA_ONLY,ALL_FILES}
 public static class GroupUiInfo{public String getGroupName(){return "android.permission-group.CAMERA";}public String getPersistentDeviceId(){return "default";}public Subtitle getSubtitle(){return Subtitle.NONE;}}
}''',
'com/android/permissioncontroller/permission/ui/model/AppPermissionViewModel.java':'''package com.android.permissioncontroller.permission.ui.model;
import java.util.*;import androidx.lifecycle.*;
public class AppPermissionViewModel{
 public enum ButtonType{ALLOW,ALLOW_ALWAYS,ALLOW_FOREGROUND,ASK,ASK_ONCE,DENY,DENY_FOREGROUND,LOCATION_ACCURACY}
 public static class ButtonState{public boolean isShown(){return true;}public boolean isChecked(){return false;}public boolean isEnabled(){return true;}}
 public static boolean initialNull;public static AppPermissionViewModel last;
 final TestLiveData<Map<ButtonType,ButtonState>> live=new TestLiveData<>();final TestLiveData<kotlin.Pair<Integer,Integer>> detail=new TestLiveData<>();
 public AppPermissionViewModel(android.app.Application a,String p,String g,android.os.UserHandle u,long id,String device){last=this;live.stale=true;live.value=initialNull?null:choices();}
 public static Map<ButtonType,ButtonState> choices(){return Collections.singletonMap(ButtonType.ALLOW_FOREGROUND,new ButtonState());}
 public TestLiveData<Map<ButtonType,ButtonState>> getButtonStateLiveData(){return live;}
 public TestLiveData<kotlin.Pair<Integer,Integer>> getDetailResIdLiveData(){return detail;}
}''',
})
class RuntimePermissionModelTest(unittest.TestCase):
 def test_native_static_rationale_readiness_and_observer_retirement(self):
  jdk=os.environ.get('JAVA_HOME');javac=str(Path(jdk)/'bin/javac') if jdk else shutil.which('javac');java=str(Path(jdk)/'bin/java') if jdk else shutil.which('java')
  if not javac or not java:self.skipTest('JDK required')
  with tempfile.TemporaryDirectory() as output:
   out=Path(output);sources=[]
   for name,body in STUBS.items():
    path=out/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(body);sources.append(path)
   sources += [ROOT/'vendor/octosense/settings/src/dev/makepad/octosense/permissions/PermissionsSettingsContract.java',ROOT/'home/android/platform-build/permissioncontroller/files/src/com/android/permissioncontroller/octosense/NativePermissionModel.java',ROOT/'tests/java/RuntimePermissionModelTest.java']
   result=subprocess.run([javac,'-d',output,*map(str,sources)],capture_output=True,text=True);self.assertEqual(result.returncode,0,result.stdout+result.stderr)
   result=subprocess.run([java,'-cp',output,'com.android.permissioncontroller.octosense.RuntimePermissionModelTest'],capture_output=True,text=True,timeout=15);self.assertEqual(result.returncode,0,result.stdout+result.stderr)
