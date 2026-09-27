"""Exercise native-role observation/confirmation authority with deterministic services."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from test_app_notifications_backend import STUBS as NOTIFICATION_STUBS

ROOT = Path(__file__).resolve().parents[1]
STUBS = {key: NOTIFICATION_STUBS[key] for key in [
    'org/json/JSONObject.java', 'org/json/JSONArray.java',
    'android/content/pm/SigningInfo.java', 'android/content/pm/Signature.java',
    'android/app/KeyguardManager.java', 'android/os/SystemClock.java',
]}
STUBS.update({
    'android/os/UserHandle.java': '''package android.os; public final class UserHandle {
        public static final UserHandle SYSTEM=new UserHandle(0); public final int id;
        public UserHandle(int id){this.id=id;}public static UserHandle getUserHandleForUid(int uid){return new UserHandle(uid/100000);}
        public boolean equals(Object other){return other instanceof UserHandle&&id==((UserHandle)other).id;}
        public int hashCode(){return id;}}
    ''',
    'android/os/Process.java': '''package android.os; public final class Process {
        public static int user;public static UserHandle myUserHandle(){return new UserHandle(user);}}
    ''',
    'android/os/UserManager.java': '''package android.os; public final class UserManager {
        public boolean foreground=true,unlocked=true;public boolean isUserForeground(){return foreground;}
        public boolean isUserUnlocked(){return unlocked;}}
    ''',
    'android/content/Intent.java': '''package android.content; public final class Intent {
        public static final int FLAG_ACTIVITY_NEW_TASK=1,FLAG_ACTIVITY_MULTIPLE_TASK=2,FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS=4;
        public String identifier,ticket;public int flags;public final Class<?> destination;
        public Intent(Context c,Class<?> d){destination=d;}public Intent setIdentifier(String value){identifier=value;return this;}
        public Intent putExtra(String key,String value){ticket=value;return this;}public Intent addFlags(int value){flags|=value;return this;}}
    ''',
    'android/app/PendingIntent.java': '''package android.app; public final class PendingIntent {
        public static final int FLAG_IMMUTABLE=1,FLAG_ONE_SHOT=2;public final android.content.Intent intent;public final int flags;
        private PendingIntent(android.content.Intent i,int f){intent=i;flags=f;}
        public static PendingIntent getActivity(android.content.Context c,int n,android.content.Intent i,int f){return new PendingIntent(i,f);}}
    ''',
    'android/app/role/RoleManager.java': '''package android.app.role; public final class RoleManager {
        public final java.util.Map<String,java.util.List<String>> holders=new java.util.HashMap<>();
        public java.util.List<String> getRoleHolders(String role){return holders.getOrDefault(role,java.util.Collections.emptyList());}}
    ''',
    'android/content/Context.java': '''package android.content; public final class Context {
        public final android.os.UserManager users=new android.os.UserManager();
        public final android.app.KeyguardManager lock=new android.app.KeyguardManager();
        public final android.app.role.RoleManager roles=new android.app.role.RoleManager();
        public final android.content.pm.PackageManager packages=new android.content.pm.PackageManager();
        public <T>T getSystemService(Class<T> type){return type.cast(type==android.os.UserManager.class?users:type==android.app.role.RoleManager.class?roles:lock);}
        public Context getApplicationContext(){return this;}public android.content.pm.PackageManager getPackageManager(){return packages;}
        public String getString(int resource){return "Native role";}}
    ''',
    'android/content/pm/PackageManager.java': '''package android.content.pm; public final class PackageManager {
        public static final int GET_SIGNING_CERTIFICATES=1;public final java.util.Map<String,PackageInfo> apps=new java.util.HashMap<>();
        public PackageInfo getPackageInfo(String pkg,int flags)throws NameNotFoundException {PackageInfo info=apps.get(pkg);if(info==null)throw new NameNotFoundException();return info;}
        public static final class NameNotFoundException extends Exception {}}
    ''',
    'android/content/pm/PackageInfo.java': '''package android.content.pm; public final class PackageInfo {
        public String packageName;public ApplicationInfo applicationInfo=new ApplicationInfo();public SigningInfo signingInfo=new SigningInfo();
        public long firstInstallTime=1,lastUpdateTime=2;public long getLongVersionCode(){return 1;}}
    ''',
    'android/content/pm/ApplicationInfo.java': '''package android.content.pm; public final class ApplicationInfo {
        public static final int FLAG_INSTALLED=1,FLAG_SUSPENDED=2;public int uid=10123,flags=FLAG_INSTALLED;public boolean enabled=true;
        public String packageName;public CharSequence label;public CharSequence loadLabel(PackageManager pm){return label==null?packageName:label;}}
    ''',
    'com/android/role/controller/model/Roles.java': '''package com.android.role.controller.model; public final class Roles {
        public static final Roles INSTANCE=new Roles();public final java.util.Map<String,Role> roles=new java.util.HashMap<>();
        public static Roles get(android.content.Context context){return INSTANCE;}public Role get(String name){return roles.get(name);}}
    ''',
    'com/android/role/controller/model/Role.java': '''package com.android.role.controller.model; public final class Role {
        public boolean available=true,visible=true,exclusive=true,restricted,none=true;public final java.util.List<String> candidates=new java.util.ArrayList<>();
        public final java.util.Set<String> hidden=new java.util.HashSet<>(),denied=new java.util.HashSet<>();
        public int getLabelResource(){return 1;}public boolean isAvailableAsUser(android.os.UserHandle u,android.content.Context c){return available;}
        public boolean isVisibleAsUser(android.os.UserHandle u,android.content.Context c){return visible;}public boolean isExclusive(){return exclusive;}
        public Object getRestrictionIntentAsUser(android.os.UserHandle u,android.content.Context c){return restricted?new Object():null;}
        public java.util.List<String> getQualifyingPackagesAsUser(android.os.UserHandle u,android.content.Context c){return candidates;}
        public boolean isApplicationVisibleAsUser(android.content.pm.ApplicationInfo a,android.os.UserHandle u,android.content.Context c){return !hidden.contains(a.packageName);}
        public Object getApplicationRestrictionIntentAsUser(android.content.pm.ApplicationInfo a,android.os.UserHandle u,android.content.Context c){return denied.contains(a.packageName)?new Object():null;}
        public boolean shouldShowNone(){return none;}}
    ''',
    'com/android/permissioncontroller/octosense/OctoSenseRoleConfirmationActivity.java':
        'package com.android.permissioncontroller.octosense;public final class OctoSenseRoleConfirmationActivity {}',
})


class DefaultRolesBackendTest(unittest.TestCase):
    def test_platform_review_requires_fresh_observed_identity_policy_and_owner(self):
        jdk = os.environ.get('JAVA_HOME')
        javac = str(Path(jdk) / 'bin/javac') if jdk else shutil.which('javac')
        java = str(Path(jdk) / 'bin/java') if jdk else shutil.which('java')
        if not javac or not java:
            self.skipTest('JDK required')
        with tempfile.TemporaryDirectory() as output:
            sources = []
            for name, content in STUBS.items():
                path = Path(output) / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content)
                sources.append(path)
            sources += [ROOT / 'vendor/octosense/settings/src/dev/makepad/octosense/roles/RolesSettingsContract.java',
                        ROOT / 'home/android/platform-build/permissioncontroller/files/src/com/android/permissioncontroller/octosense/RoleSettingsBackend.java',
                        ROOT / 'tests/java/DefaultRolesBackendTest.java']
            result = subprocess.run([javac, '-d', output, *map(str, sources)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            result = subprocess.run([java, '-cp', output, 'com.android.permissioncontroller.octosense.DefaultRolesBackendTest'], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
