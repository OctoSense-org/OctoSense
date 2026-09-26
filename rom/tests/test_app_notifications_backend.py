"""Exercise the production Broker backend against deterministic notification services."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]

# These stubs model service data, not the backend's filtering/authority decisions.
STUBS = {
    "android/Manifest.java": """package android; public final class Manifest {
        public static final class permission {public static final String POST_NOTIFICATIONS="post";}}
    """,
    "android/os/Parcelable.java": """package android.os; public interface Parcelable {
        void writeToParcel(Parcel p,int flags); interface Creator<T>{T createFromParcel(Parcel p);}}
    """,
    "android/os/Parcel.java": """package android.os; public final class Parcel {
        public Object value; public static Parcel obtain(){return new Parcel();}
        public void setDataPosition(int n){} public void recycle(){}
        public byte[] marshall(){return value.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8);}}
    """,
    "android/os/Process.java": """package android.os; public final class Process {
        public static final int SYSTEM_UID=1000; public static int uid=SYSTEM_UID;
        public static int myUid(){return uid;}}
    """,
    "android/os/UserHandle.java": """package android.os; public final class UserHandle {
        public static final UserHandle SYSTEM=new UserHandle(); public static int myUserId(){return 0;}
        public static int getUserId(int uid){return uid/100000;}}
    """,
    "android/os/SystemClock.java": """package android.os; public final class SystemClock {
        public static long now=100; public static long elapsedRealtime(){return now;}}
    """,
    "android/os/UserManager.java": """package android.os; public final class UserManager {
        public static final String DISALLOW_APPS_CONTROL="no_apps_control";
        public boolean unlocked=true,admin=true,restricted;
        public boolean isUserUnlocked(){return unlocked;} public boolean isAdminUser(){return admin;}
        public boolean hasUserRestriction(String key){return restricted;}}
    """,
    "android/app/ActivityManager.java": """package android.app; public final class ActivityManager {
        public static int user; public static int getCurrentUser(){return user;}}
    """,
    "android/app/KeyguardManager.java": """package android.app; public final class KeyguardManager {
        public boolean locked; public boolean isKeyguardLocked(){return locked;}}
    """,
    "android/content/Context.java": """package android.content; public final class Context {
        public final android.os.UserManager users=new android.os.UserManager();
        public final android.app.KeyguardManager lock=new android.app.KeyguardManager();
        public final android.content.pm.PackageManager packages=new android.content.pm.PackageManager();
        public <T>T getSystemService(Class<T> type){return type.cast(type==android.os.UserManager.class?users:lock);}
        public android.content.pm.PackageManager getPackageManager(){return packages;}}
    """,
    "android/content/pm/PackageManager.java": """package android.content.pm; public final class PackageManager {
        public static final int GET_PERMISSIONS=1,GET_SIGNING_CERTIFICATES=2;
        public static final int FLAG_PERMISSION_SYSTEM_FIXED=1,FLAG_PERMISSION_POLICY_FIXED=2;
        public PackageInfo info=new PackageInfo(); public int flags; public boolean suspended;
        public PackageInfo getPackageInfo(String p,int f)throws NameNotFoundException {
            if(info==null)throw new NameNotFoundException();return info;}
        public int getPermissionFlags(String p,String pkg,android.os.UserHandle user){return flags;}
        public boolean isPackageSuspended(String pkg){return suspended;}
        public static final class NameNotFoundException extends Exception {}}
    """,
    "android/content/pm/PackageInfo.java": """package android.content.pm; public final class PackageInfo {
        public ApplicationInfo applicationInfo=new ApplicationInfo();
        public String[] requestedPermissions={android.Manifest.permission.POST_NOTIFICATIONS};
        public SigningInfo signingInfo=new SigningInfo(); public long firstInstallTime=1,lastUpdateTime=2;
        public long getLongVersionCode(){return 1;}}
    """,
    "android/content/pm/ApplicationInfo.java": """package android.content.pm; public final class ApplicationInfo {
        public int uid=10123,targetSdkVersion=25; public CharSequence loadLabel(PackageManager pm){return "Fixture";}}
    """,
    "android/content/pm/SigningInfo.java": """package android.content.pm; public final class SigningInfo {
        public Signature[] getApkContentsSigners(){return new Signature[]{new Signature()};}}
    """,
    "android/content/pm/Signature.java": """package android.content.pm; public final class Signature {
        public String toCharsString(){return "fixture-signer";}}
    """,
    "android/content/pm/ParceledListSlice.java": """package android.content.pm;
        public final class ParceledListSlice<T>{private final java.util.List<T> list;
        public ParceledListSlice(java.util.List<T> list){this.list=list;}public java.util.List<T> getList(){return list;}}
    """,
    "android/net/Uri.java": """package android.net; public final class Uri {
        public static final Uri EMPTY=new Uri(""); private final String value;
        public Uri(String value){this.value=value;}public String toString(){return value;}
        public boolean equals(Object other){return other instanceof Uri&&value.equals(((Uri)other).value);}
        public int hashCode(){return value.hashCode();}}
    """,
    "android/provider/Settings.java": """package android.provider; public final class Settings {
        public static final class System{public static final android.net.Uri DEFAULT_NOTIFICATION_URI=new android.net.Uri("default");}}
    """,
    "android/util/Base64.java": """package android.util; public final class Base64 {
        public static final int NO_WRAP=2;public static String encodeToString(byte[] bytes,int flags){
            return java.util.Base64.getEncoder().encodeToString(bytes);}}
    """,
    "android/app/NotificationChannel.java": """package android.app; public final class NotificationChannel implements android.os.Parcelable {
        public static final String DEFAULT_CHANNEL_ID="miscellaneous";public static final int USER_LOCKED_IMPORTANCE=4,USER_LOCKED_SOUND=32;
        private String id,name,group;private int importance,original,locks;private boolean deleted,blockable;
        private android.net.Uri sound;private Object attributes;
        public NotificationChannel(String id,String name,int importance){this.id=id;this.name=name;this.importance=importance;original=importance;}
        private NotificationChannel(NotificationChannel x){id=x.id;name=x.name;group=x.group;importance=x.importance;original=x.original;locks=x.locks;deleted=x.deleted;blockable=x.blockable;sound=x.sound;attributes=x.attributes;}
        public String getId(){return id;}public String getName(){return name;}public String getGroup(){return group;}public void setGroup(String v){group=v;}
        public int getImportance(){return importance;}public void setImportance(int v){importance=v;}public int getOriginalImportance(){return original;}
        public int getUserLockedFields(){return locks;}public void lockFields(int v){locks|=v;}
        public boolean isDeleted(){return deleted;}public boolean isBlockable(){return blockable;}
        public android.net.Uri getSound(){return sound;}public Object getAudioAttributes(){return attributes;}
        public void setSound(android.net.Uri value,Object attributes){sound=value;this.attributes=attributes;}
        public void writeToParcel(android.os.Parcel p,int flags){p.value=new NotificationChannel(this);}
        public static final android.os.Parcelable.Creator<NotificationChannel> CREATOR=p->new NotificationChannel((NotificationChannel)p.value);
        public String toString(){return id+":"+name+":"+group+":"+importance+":"+original+":"+locks+":"+sound+":"+deleted+":"+blockable;}}
    """,
    "android/app/NotificationChannelGroup.java": """package android.app; public final class NotificationChannelGroup implements android.os.Parcelable {
        private final String id,name;private boolean blocked;
        public NotificationChannelGroup(String id,String name){this.id=id;this.name=name;}
        private NotificationChannelGroup(NotificationChannelGroup x){id=x.id;name=x.name;blocked=x.blocked;}
        public String getId(){return id;}public String getName(){return name;}public boolean isBlocked(){return blocked;}public void setBlocked(boolean v){blocked=v;}
        public void writeToParcel(android.os.Parcel p,int flags){p.value=new NotificationChannelGroup(this);}
        public static final android.os.Parcelable.Creator<NotificationChannelGroup> CREATOR=p->new NotificationChannelGroup((NotificationChannelGroup)p.value);
        public String toString(){return id+":"+name+":"+blocked;}}
    """,
    "android/app/NotificationManager.java": """package android.app; public final class NotificationManager {
        public static final int IMPORTANCE_NONE=0,IMPORTANCE_LOW=2,IMPORTANCE_DEFAULT=3,IMPORTANCE_UNSPECIFIED=-1000;
        public static INotificationManager service;public static INotificationManager getService(){return service;}}
    """,
    "android/app/INotificationManager.java": """package android.app;
        public final class INotificationManager {
        public boolean enabled=true,locked,onlyDefault=true;public int appWrites,channelWrites,groupWrites;
        public final java.util.List<NotificationChannelGroup> groups=new java.util.ArrayList<>();
        public final java.util.List<NotificationChannel> channels=new java.util.ArrayList<>();
        public boolean areNotificationsEnabledForPackage(String pkg,int uid){return enabled;}
        public boolean isImportanceLocked(String pkg,int uid){return locked;}
        public boolean onlyHasDefaultChannel(String pkg,int uid){return onlyDefault;}
        public android.content.pm.ParceledListSlice<NotificationChannelGroup> getNotificationChannelGroupsForPackage(String p,int u,boolean d){
            return new android.content.pm.ParceledListSlice<>(java.util.Collections.unmodifiableList(groups));}
        public android.content.pm.ParceledListSlice<NotificationChannel> getNotificationChannelsForPackage(String p,int u,boolean d){return new android.content.pm.ParceledListSlice<>(channels);}
        public void setNotificationsEnabledForPackage(String pkg,int uid,boolean value){appWrites++;enabled=value;}
        public void updateNotificationChannelForPackage(String pkg,int uid,NotificationChannel value){channelWrites++;
            for(int i=0;i<channels.size();i++)if(channels.get(i).getId().equals(value.getId())){channels.set(i,value);return;}throw new IllegalArgumentException();}
        public void updateNotificationChannelGroupForPackage(String pkg,int uid,NotificationChannelGroup value){groupWrites++;
            if(value.getId()==null)throw new IllegalArgumentException("Synthetic group cannot be updated");
            for(int i=0;i<groups.size();i++)if(value.getId().equals(groups.get(i).getId())){groups.set(i,value);return;}throw new IllegalArgumentException();}}
    """,
    "org/json/JSONObject.java": """package org.json;public final class JSONObject {
        public static final Object NULL=new Object();private final java.util.Map<String,Object> values=new java.util.LinkedHashMap<>();
        public JSONObject put(String k,Object v){values.put(k,v);return this;}public Object opt(String k){return values.get(k);}
        public JSONObject getJSONObject(String k){return (JSONObject)values.get(k);}public JSONArray getJSONArray(String k){return (JSONArray)values.get(k);}
        public String getString(String k){return (String)values.get(k);}public int getInt(String k){return ((Number)values.get(k)).intValue();}
        public boolean getBoolean(String k){return (Boolean)values.get(k);}public String toString(){return values.toString();}}
    """,
    "org/json/JSONArray.java": """package org.json;public final class JSONArray {
        private final java.util.List<Object> values=new java.util.ArrayList<>();public JSONArray(){}
        public JSONArray(java.util.Collection<?> items){values.addAll(items);}public JSONArray put(Object v){values.add(v);return this;}
        public int length(){return values.size();}public JSONObject getJSONObject(int i){return (JSONObject)values.get(i);}
        public String getString(int i){return (String)values.get(i);}public String toString(){return values.toString();}}
    """,
}


class AppNotificationsBackendTest(unittest.TestCase):
    def test_ungrouped_legacy_channels_and_real_group_authority(self):
        jdk = os.environ.get("JAVA_HOME")
        javac = str(Path(jdk) / "bin/javac") if jdk else shutil.which("javac")
        java = str(Path(jdk) / "bin/java") if jdk else shutil.which("java")
        if not javac or not java:
            self.skipTest("JDK required")
        with tempfile.TemporaryDirectory() as output:
            sources = []
            for name, content in STUBS.items():
                path = Path(output) / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content)
                sources.append(path)
            sources += [
                ROOT / "vendor/octosense/settings/src/dev/makepad/octosense/notifications/AppNotificationsContract.java",
                ROOT / "vendor/octosense/settings-broker/src/dev/makepad/octosense/settingsbroker/AppNotificationsBackend.java",
                ROOT / "tests/java/AppNotificationsBackendTest.java",
            ]
            result = subprocess.run([javac, "-d", output, *map(str, sources)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            result = subprocess.run([java, "-cp", output, "dev.makepad.octosense.settingsbroker.AppNotificationsBackendTest"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
