package dev.makepad.octosense.settingsbroker;

import android.app.ActivityManager;
import android.app.KeyguardManager;
import android.app.PendingIntent;
import android.app.admin.DevicePolicyManager;
import android.app.usage.StorageStats;
import android.app.usage.StorageStatsManager;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.content.pm.ApplicationInfo;
import android.content.pm.IPackageDataObserver;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.os.Process;
import android.os.UserHandle;
import android.os.UserManager;
import dev.makepad.octosense.appstorage.AppStorageBackend;
import dev.makepad.octosense.appstorage.AppStorageContract;
import java.util.Arrays;
import java.util.Objects;

/** Native package-manager operations, never filesystem deletion or permission-reset bypasses. */
final class AppStoragePlatform implements AppStorageBackend.Platform {
    private final Context context;
    AppStoragePlatform(Context context){this.context=context;}
    private static final class State extends AppStorageBackend.State {int uid;ComponentName manageSpace;}
    private boolean owner(){UserManager users=context.getSystemService(UserManager.class);KeyguardManager lock=context.getSystemService(KeyguardManager.class);return Process.myUid()==Process.SYSTEM_UID&&UserHandle.myUserId()==0&&ActivityManager.getCurrentUser()==0&&users!=null&&users.isUserUnlocked()&&lock!=null&&!lock.isKeyguardLocked();}
    private static String label(CharSequence value,String fallback){StringBuilder out=new StringBuilder();if(value!=null)value.toString().codePoints().filter(c->!Character.isISOControl(c)).limit(256).forEach(out::appendCodePoint);return out.toString().trim().isEmpty()?fallback:out.toString();}
    private static boolean critical(String pkg){return Arrays.asList("android","com.android.settings","com.android.systemui","com.android.permissioncontroller","com.google.android.permissioncontroller","dev.makepad.octosense","dev.makepad.octosense.agent","dev.makepad.octosense.settingsbroker","dev.makepad.octosense.bridge","dev.makepad.octosense.quickstep","com.android.launcher3","com.google.android.apps.nexuslauncher").contains(pkg);}
    @Override public AppStorageBackend.State read(String pkg)throws Exception{
        AppStorageContract.packageName(pkg);State out=new State();out.packageName=pkg;
        if(!owner()){out.availability="restricted";out.reason="locked";return out;}
        PackageManager pm=context.getPackageManager();PackageInfo info;
        try{info=pm.getPackageInfo(pkg,PackageManager.GET_SIGNING_CERTIFICATES);}catch(PackageManager.NameNotFoundException missing){out.availability="missing";out.reason="app_missing";return out;}
        ApplicationInfo app=info.applicationInfo;if(app==null||UserHandle.getUserId(app.uid)!=0)throw new SecurityException("Wrong storage package user");out.uid=app.uid;out.label=label(app.loadLabel(pm),pkg);
        String[] peers=pm.getPackagesForUid(app.uid);if(peers==null||!Arrays.asList(peers).contains(pkg))throw new SecurityException("Wrong storage package UID");Arrays.sort(peers);out.sharedUid=peers.length!=1;
        if(info.signingInfo==null)throw new IllegalStateException("Unknown package signer");StringBuilder signer=new StringBuilder();for(android.content.pm.Signature s:info.signingInfo.getApkContentsSigners())signer.append(AppStorageContract.hash(s.toCharsString()));
        android.system.StructStat source=android.system.Os.stat(app.sourceDir);
        out.incarnation=AppStorageContract.hash(pkg,String.valueOf(app.uid),String.valueOf(info.firstInstallTime),String.valueOf(info.lastUpdateTime),String.valueOf(info.getLongVersionCode()),app.sourceDir,String.valueOf(source.st_dev),String.valueOf(source.st_ino),String.valueOf(app.storageUuid),signer.toString());
        DevicePolicyManager dpm=context.getSystemService(DevicePolicyManager.class);UserManager users=context.getSystemService(UserManager.class);StorageStatsManager stats=context.getSystemService(StorageStatsManager.class);
        if(dpm==null||users==null||stats==null)return out;
        boolean restricted=!users.isAdminUser()||users.hasUserRestriction(UserManager.DISALLOW_APPS_CONTROL);
        boolean module;try{pm.getModuleInfo(pkg,0);module=true;}catch(PackageManager.NameNotFoundException ordinary){module=false;}
        boolean protectedApp=critical(pkg)||UserHandle.getAppId(app.uid)<Process.FIRST_APPLICATION_UID||dpm.packageHasActiveAdmins(pkg)||pm.isPackageStateProtected(pkg,0)||module||(app.flags&ApplicationInfo.FLAG_PERSISTENT)!=0;
        boolean cannotClearSystem=(app.flags&(ApplicationInfo.FLAG_SYSTEM|ApplicationInfo.FLAG_ALLOW_CLEAR_USER_DATA))==ApplicationInfo.FLAG_SYSTEM;
        boolean unavailable=app.isInstantApp()||pm.isPackageSuspended(pkg)||(app.flags&ApplicationInfo.FLAG_INSTALLED)==0;
        boolean clearDataAllowed=!protectedApp&&!cannotClearSystem;
        if(app.manageSpaceActivityName!=null){
            ComponentName component=new ComponentName(pkg,app.manageSpaceActivityName);
            try{ActivityInfo activity=pm.getActivityInfo(component,0);
                if(activity.enabled&&activity.exported&&activity.applicationInfo.uid==app.uid&&pkg.equals(activity.packageName)
                        &&(activity.permission==null||context.checkSelfPermission(activity.permission)==PackageManager.PERMISSION_GRANTED))out.manageSpace=component;
            }catch(PackageManager.NameNotFoundException ignored){}
        }
        try{StorageStats actual=stats.queryStatsForPackage(app.storageUuid,pkg,UserHandle.SYSTEM);out.appBytes=actual.getAppBytes();out.dataBytes=actual.getDataBytes();out.cacheBytes=actual.getCacheBytes();}catch(java.io.IOException|SecurityException unknown){/* Unknown sizes never become zero or authorize deletion. */}
        out.availability="available";out.reason=restricted?"device_policy":protectedApp?"protected_app":out.sharedUid?"shared_uid":unavailable?"service_unavailable":!out.statsAvailable()?"stats_unavailable":"none";
        if(!restricted&&!protectedApp&&!out.sharedUid&&!unavailable&&out.statsAvailable()){
            if(out.cacheBytes>0)out.actions.add(AppStorageContract.Action.CLEAR_CACHE);
            if(out.dataBytes>out.cacheBytes){
                if(app.manageSpaceActivityName!=null){if(out.manageSpace!=null)out.actions.add(AppStorageContract.Action.MANAGE_SPACE);}
                else if(clearDataAllowed)out.actions.add(AppStorageContract.Action.CLEAR_DATA);
            }
        }
        // Review authorizes clearing this app's current cache/data, not deleting an exact
        // byte count. Running apps can write while the user reads the confirmation.
        // Sizes remain observations; fresh stats and eligible actions are checked again
        // before enqueue. Package replacement, policy and target changes retire review.
        out.revision=AppStorageContract.hash(out.incarnation,Arrays.toString(peers),String.valueOf(restricted),String.valueOf(protectedApp),String.valueOf(cannotClearSystem),String.valueOf(unavailable),String.valueOf(out.manageSpace),String.valueOf(out.statsAvailable()),out.actions.toString());
        if(!owner()){State retired=new State();retired.availability="restricted";retired.reason="locked";return retired;}return out;
    }
    @Override public AppStorageBackend.Started request(AppStorageBackend.State raw,AppStorageContract.Action action,AppStorageBackend.Completion completion)throws Exception{
        State before=(State)raw;State current=(State)read(before.packageName);
        if(!"available".equals(current.availability)||!current.actions.contains(action))throw new AppStorageBackend.Rejected(AppStorageBackend.Failure.RESTRICTED);
        if(!Objects.equals(current.incarnation,before.incarnation)||!Objects.equals(current.revision,before.revision))throw new AppStorageBackend.Rejected(AppStorageBackend.Failure.TARGET_CHANGED);
        if(action==AppStorageContract.Action.MANAGE_SPACE){
            if(current.manageSpace==null)throw new AppStorageBackend.Rejected(AppStorageBackend.Failure.UNAVAILABLE);
            Intent intent=new Intent(Intent.ACTION_DEFAULT).setComponent(current.manageSpace).setIdentifier("octosense-storage-"+java.util.UUID.randomUUID())
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_MULTIPLE_TASK|Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS);
            PendingIntent flow=PendingIntent.getActivity(context,0,intent,PendingIntent.FLAG_ONE_SHOT|PendingIntent.FLAG_IMMUTABLE);
            return new AppStorageBackend.Started(true,flow);
        }
        IPackageDataObserver observer=new IPackageDataObserver.Stub(){@Override public void onRemoveCompleted(String packageName,boolean succeeded){completion.complete(packageName,succeeded);}};
        if(action==AppStorageContract.Action.CLEAR_CACHE){context.getPackageManager().deleteApplicationCacheFiles(before.packageName,observer);return new AppStorageBackend.Started(true,null);}
        // This overload preserves native permission, notification, URI, job and alarm resets.
        ActivityManager activity=context.getSystemService(ActivityManager.class);
        if(activity==null)return new AppStorageBackend.Started(false,null);
        return new AppStorageBackend.Started(activity.clearApplicationUserData(before.packageName,observer),null);
    }
}
