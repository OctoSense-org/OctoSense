package dev.makepad.octosense.settingsbroker;

import android.app.ActivityManager;
import android.app.AppOpsManager;
import android.app.KeyguardManager;
import android.app.admin.DevicePolicyManager;
import android.app.role.RoleManager;
import android.content.Context;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.content.res.Resources;
import android.os.IDeviceIdleController;
import android.os.Process;
import android.os.ServiceManager;
import android.os.UserHandle;
import android.os.UserManager;
import android.provider.DeviceConfig;
import android.telecom.TelecomManager;
import dev.makepad.octosense.battery.AppBatteryBackend;
import dev.makepad.octosense.battery.AppBatteryContract;
import java.util.Arrays;
import java.util.List;
import java.util.Objects;

/** Mirrors native BatteryOptimizeUtils policy and framework side effects, not Settings telemetry. */
final class AppBatteryPlatform implements AppBatteryBackend.Platform {
    private final Context context;
    AppBatteryPlatform(Context context){this.context=context;}
    private boolean owner(){UserManager users=context.getSystemService(UserManager.class);KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        return Process.myUid()==Process.SYSTEM_UID&&UserHandle.myUserId()==0&&ActivityManager.getCurrentUser()==0
            &&users!=null&&users.isUserUnlocked()&&lock!=null&&!lock.isKeyguardLocked();}
    private IDeviceIdleController idle(){return IDeviceIdleController.Stub.asInterface(ServiceManager.getService("deviceidle"));}
    private static final class State extends AppBatteryBackend.State {
        int uid;boolean restrictionApi;String authority;
    }
    private String text(CharSequence value,String fallback){StringBuilder result=new StringBuilder();if(value!=null)value.toString().codePoints().filter(c->!Character.isISOControl(c)).limit(256).forEach(result::appendCodePoint);return result.toString().trim().isEmpty()?fallback:result.toString();}
    private static boolean contains(String[] values,String pkg){if(values==null)throw new IllegalStateException("Missing native power list");return Arrays.asList(values).contains(pkg);}
    private boolean forced(Resources resources,String name,String pkg){int id=resources.getIdentifier(name,"array","com.android.settings");if(id==0)throw new IllegalStateException("Missing native battery policy resource");return contains(resources.getStringArray(id),pkg);}
    private static Boolean restrictionApi(){try{return (Boolean)android.app.Flags.class.getMethod("appRestrictionsApi").invoke(null);}catch(ReflectiveOperationException|LinkageError unavailable){return null;}}
    @Override public AppBatteryBackend.State read(String pkg)throws Exception {
        AppBatteryContract.packageName(pkg);State state=new State();state.packageName=pkg;
        if(!owner()){state.availability="restricted";state.reason="locked";return state;}
        PackageManager pm=context.getPackageManager();PackageInfo info;
        try{info=pm.getPackageInfo(pkg,PackageManager.GET_SIGNING_CERTIFICATES);}catch(PackageManager.NameNotFoundException missing){state.availability="missing";state.reason="app_missing";return state;}
        ApplicationInfo app=info.applicationInfo;
        if(app==null||UserHandle.getUserId(app.uid)!=0)throw new SecurityException("Wrong package user");
        state.uid=app.uid;state.preO=app.targetSdkVersion<26;state.label=text(app.loadLabel(pm),pkg);
        StringBuilder signer=new StringBuilder();if(info.signingInfo==null)throw new IllegalStateException("Unknown package signer");
        for(android.content.pm.Signature signature:info.signingInfo.getApkContentsSigners())signer.append(AppBatteryContract.hash(signature.toCharsString()));
        android.system.StructStat source=android.system.Os.stat(app.sourceDir);
        state.incarnation=AppBatteryContract.hash(pkg,String.valueOf(app.uid),String.valueOf(info.firstInstallTime),String.valueOf(info.lastUpdateTime),String.valueOf(info.getLongVersionCode()),app.sourceDir,String.valueOf(source.st_dev),String.valueOf(source.st_ino),signer.toString());
        String[] peers=pm.getPackagesForUid(app.uid);if(peers==null||!Arrays.asList(peers).contains(pkg))throw new SecurityException("UID package mismatch");
        Arrays.sort(peers);state.sharedUid=peers.length!=1;
        IDeviceIdleController idle=idle();AppOpsManager ops=context.getSystemService(AppOpsManager.class);DevicePolicyManager policy=context.getSystemService(DevicePolicyManager.class);UserManager users=context.getSystemService(UserManager.class);
        if(idle==null||ops==null||policy==null||users==null)return state;
        boolean full=contains(idle.getFullPowerWhitelist(),pkg),system=contains(idle.getSystemPowerWhitelist(),pkg);
        boolean activeAdmin=policy.packageHasActiveAdmins(pkg),protectedPackage=pm.isPackageStateProtected(pkg,0);
        boolean exemptFlag=DeviceConfig.getBoolean(DeviceConfig.NAMESPACE_ACTIVITY_MANAGER,"system_exempt_power_restrictions_enabled",true);
        boolean exempt=exemptFlag&&ops.checkOpNoThrow(AppOpsManager.OP_SYSTEM_EXEMPT_FROM_POWER_RESTRICTIONS,app.uid,pkg)==AppOpsManager.MODE_ALLOWED;
        boolean defaultApp=false;
        if(pm.hasSystemFeature(PackageManager.FEATURE_TELEPHONY)){
            RoleManager roles=context.getSystemService(RoleManager.class);TelecomManager telecom=context.getSystemService(TelecomManager.class);
            if(roles==null||telecom==null)throw new IllegalStateException("Default phone role unavailable");
            List<String> sms=roles.getRoleHolders(RoleManager.ROLE_SMS);
            defaultApp=sms.contains(pkg)||pkg.equals(telecom.getDefaultDialerPackage());
        }
        ApplicationInfo settings=pm.getApplicationInfo("com.android.settings",0);
        if((settings.flags&(ApplicationInfo.FLAG_SYSTEM|ApplicationInfo.FLAG_UPDATED_SYSTEM_APP))==0||pm.checkSignatures("android","com.android.settings")!=PackageManager.SIGNATURE_MATCH)throw new SecurityException("Untrusted native battery policy resources");
        Resources nativeResources=pm.getResourcesForApplication(settings);
        boolean forceOptimize=forced(nativeResources,"config_force_battery_optimize_mode_apps",pkg);
        boolean forceUnrestrict=forced(nativeResources,"config_force_battery_unrestrict_mode_apps",pkg);
        boolean policyRestricted=!users.isAdminUser()||users.hasUserRestriction(UserManager.DISALLOW_APPS_CONTROL);
        boolean unavailableApp=pm.isPackageSuspended(pkg)||app.isInstantApp()||(app.flags&ApplicationInfo.FLAG_INSTALLED)==0;
        Boolean restrictionApi=restrictionApi();state.restrictionApi=Boolean.TRUE.equals(restrictionApi);
        // Evaluated checks translate MODE_FOREGROUND to allowed/ignored based
        // on process state. Bind the review to the stored policy instead, so
        // a custom mode cannot masquerade as Restricted or evade stale checks.
        state.runAny=ops.unsafeCheckOpRawNoThrow(AppOpsManager.OP_RUN_ANY_IN_BACKGROUND,app.uid,pkg);
        if(state.preO)state.runIn=ops.unsafeCheckOpRawNoThrow(AppOpsManager.OP_RUN_IN_BACKGROUND,app.uid,pkg);
        state.allowlisted=full||defaultApp||activeAdmin||exempt||protectedPackage;
        boolean protectedApp=system||forceOptimize||forceUnrestrict||defaultApp||activeAdmin||exempt||protectedPackage||UserHandle.getAppId(app.uid)<Process.FIRST_APPLICATION_UID;
        state.canSet=!policyRestricted&&!protectedApp&&!state.sharedUid&&!unavailableApp&&restrictionApi!=null;
        state.availability="available";
        state.reason=policyRestricted?"device_policy":protectedApp?"protected_app":state.sharedUid?"shared_uid":unavailableApp||restrictionApi==null?"service_unavailable":"none";
        state.authority=AppBatteryContract.hash(state.incarnation,Arrays.toString(peers),String.valueOf(policyRestricted),String.valueOf(protectedApp),String.valueOf(unavailableApp),String.valueOf(restrictionApi),String.valueOf(app.targetSdkVersion));
        state.revision=AppBatteryContract.hash(state.authority,String.valueOf(state.runAny),String.valueOf(state.runIn),String.valueOf(state.allowlisted));
        if(!owner()){State retired=new State();retired.availability="restricted";retired.reason="locked";return retired;}return state;
    }
    private State current(State original,Integer expectedAny,Integer expectedIn,boolean expectedAllow)throws Exception {
        State current=(State)read(original.packageName);
        if(!current.writable()||!Objects.equals(current.incarnation,original.incarnation)||!Objects.equals(current.authority,original.authority)
                ||!Objects.equals(current.runAny,expectedAny)||!Objects.equals(current.runIn,expectedIn)||current.allowlisted!=expectedAllow)return null;
        return current;
    }
    @Override public AppBatteryBackend.Write apply(AppBatteryBackend.State raw,AppBatteryContract.Mode mode)throws Exception {
        State before=(State)raw;int target=mode==AppBatteryContract.Mode.RESTRICTED?AppOpsManager.MODE_IGNORED:AppOpsManager.MODE_ALLOWED;
        boolean allow=mode==AppBatteryContract.Mode.UNRESTRICTED,wrote=false;Integer expectedAny=before.runAny,expectedIn=before.runIn;boolean expectedAllow=before.allowlisted;
        try{
            if(current(before,expectedAny,expectedIn,expectedAllow)==null)return AppBatteryBackend.Write.TARGET_CHANGED;
            AppOpsManager ops=context.getSystemService(AppOpsManager.class);ActivityManager activity=context.getSystemService(ActivityManager.class);IDeviceIdleController idle=idle();
            if(ops==null||activity==null||idle==null)return AppBatteryBackend.Write.UNAVAILABLE;
            if(before.preO&&!Objects.equals(expectedIn,target)){
                ops.setMode(AppOpsManager.OP_RUN_IN_BACKGROUND,before.uid,before.packageName,target);wrote=true;expectedIn=target;
                if(current(before,expectedAny,expectedIn,expectedAllow)==null)return AppBatteryBackend.Write.PARTIAL;
            }
            if(!Objects.equals(expectedAny,target)){
                // Native BatteryUtils records the user reason before applying the AppOp.
                activity.noteAppRestrictionEnabled(before.packageName,before.uid,ActivityManager.RESTRICTION_LEVEL_BACKGROUND_RESTRICTED,
                    target==AppOpsManager.MODE_IGNORED,ActivityManager.RESTRICTION_REASON_USER,"settings",ActivityManager.RESTRICTION_SOURCE_USER,0L);wrote=true;
                if(current(before,expectedAny,expectedIn,expectedAllow)==null)return AppBatteryBackend.Write.PARTIAL;
                ops.setMode(AppOpsManager.OP_RUN_ANY_IN_BACKGROUND,before.uid,before.packageName,target);expectedAny=target;
                if(current(before,expectedAny,expectedIn,expectedAllow)==null)return AppBatteryBackend.Write.PARTIAL;
            }
            if(allow!=expectedAllow){
                if(before.restrictionApi){activity.noteAppRestrictionEnabled(before.packageName,before.uid,ActivityManager.RESTRICTION_LEVEL_EXEMPTED,
                    allow,ActivityManager.RESTRICTION_REASON_USER,"settings",ActivityManager.RESTRICTION_SOURCE_USER,0L);wrote=true;}
                if(current(before,expectedAny,expectedIn,expectedAllow)==null)return wrote?AppBatteryBackend.Write.PARTIAL:AppBatteryBackend.Write.TARGET_CHANGED;
                if(allow)idle.addPowerSaveWhitelistApp(before.packageName);else idle.removePowerSaveWhitelistApp(before.packageName);wrote=true;
            }
            return AppBatteryBackend.Write.ACCEPTED;
        }catch(Exception|LinkageError unavailable){return wrote?AppBatteryBackend.Write.PARTIAL:AppBatteryBackend.Write.UNAVAILABLE;}
    }
}
