package dev.makepad.octosense.settingsbroker;

import android.Manifest;
import android.app.ActivityManager;
import android.app.KeyguardManager;
import android.app.admin.DevicePolicyManager;
import android.app.role.RoleManager;
import android.content.ComponentName;
import android.content.Context;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.net.NetworkPolicyManager;
import android.os.Process;
import android.os.UserHandle;
import android.os.UserManager;
import android.system.Os;
import dev.makepad.octosense.appnetwork.AppNetworkBackend;
import dev.makepad.octosense.appnetwork.AppNetworkContract.Field;
import java.util.Arrays;

/** Owner-only adapter to the pinned Settings AppDataUsage/DataSaverBackend APIs. */
final class AppNetworkPlatform implements AppNetworkBackend.Platform {
    private final Context context;
    AppNetworkPlatform(Context context){this.context=context;}
    private boolean lineage(){
        // Reflection deliberately avoids inlining Lineage constants on AOSP emulators.
        String[] names={"POLICY_REJECT_ALL","POLICY_REJECT_WIFI","POLICY_REJECT_CELLULAR","POLICY_REJECT_VPN"};
        int[] values={0x40000,0x8000,0x10000,0x20000};
        try{for(int i=0;i<names.length;i++)if(NetworkPolicyManager.class.getField(names[i]).getInt(null)!=values[i])return false;return true;}
        catch(ReflectiveOperationException|SecurityException unavailable){return false;}
    }
    private boolean protectedPackage(String pkg){
        return Arrays.asList("dev.makepad.octosense","dev.makepad.octosense.agent","dev.makepad.octosense.settingsbroker",
            "dev.makepad.octosense.bridge","dev.makepad.octosense.quickstep","com.android.systemui","com.android.settings",
            "com.android.permissioncontroller","com.google.android.permissioncontroller").contains(pkg);
    }
    private boolean meteredMutable(String pkg,DevicePolicyManager dpm,RoleManager roles){
        if(roles==null||dpm==null)return false;
        for(String role:new String[]{"android.app.role.FINANCED_DEVICE_KIOSK","android.app.role.SYSTEM_FINANCED_DEVICE_CONTROLLER"})
            if(roles.getRoleHoldersAsUser(role,UserHandle.of(0)).contains(pkg))return false;
        ComponentName owner=dpm.getProfileOwnerAsUser(0);
        if(owner==null&&dpm.getDeviceOwnerUserId()==0)owner=dpm.getDeviceOwnerComponentOnAnyUser();
        return owner==null||!dpm.isMeteredDataDisabledPackageForUser(owner,pkg,0);
    }
    @Override public AppNetworkBackend.State read(String pkg)throws Exception{
        AppNetworkBackend.State s=new AppNetworkBackend.State();
        UserManager users=context.getSystemService(UserManager.class);KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        if(Process.myUserHandle().getIdentifier()!=0||ActivityManager.getCurrentUser()!=0||users==null||!users.isAdminUser()
            ||!users.isUserUnlocked()||lock==null||lock.isKeyguardLocked()){s.availability="restricted";return s;}
        PackageManager pm=context.getPackageManager();PackageInfo info;
        try{info=pm.getPackageInfo(pkg,PackageManager.MATCH_DISABLED_COMPONENTS);}catch(PackageManager.NameNotFoundException gone){s.availability="missing";return s;}
        ApplicationInfo app=info.applicationInfo;
        if(app==null||UserHandle.getUserId(app.uid)!=0||!Process.isApplicationUid(app.uid)){s.availability="restricted";return s;}
        NetworkPolicyManager manager=context.getSystemService(NetworkPolicyManager.class);
        if(manager==null)return s;
        String[] members=pm.getPackagesForUid(app.uid);
        if(members==null||members.length==0||members.length>50)return s;
        Arrays.sort(members);s.packages=members;s.uid=app.uid;s.label=String.valueOf(app.loadLabel(pm));
        StringBuilder identity=new StringBuilder();boolean internet=false,mutable=true;
        s.meteredMutable=true;
        DevicePolicyManager dpm=context.getSystemService(DevicePolicyManager.class);RoleManager roles=context.getSystemService(RoleManager.class);
        for(String member:members){
            PackageInfo p=pm.getPackageInfo(member,PackageManager.MATCH_DISABLED_COMPONENTS);ApplicationInfo a=p.applicationInfo;
            if(a==null||a.uid!=app.uid)throw new IllegalStateException("UID membership changed");
            identity.append(member).append(':').append(a.uid).append(':').append(p.firstInstallTime).append(':').append(p.lastUpdateTime)
                .append(':').append(p.getLongVersionCode()).append(':').append(a.sourceDir).append(':').append(Os.stat(a.sourceDir).st_ino).append('\n');
            internet|=pm.checkPermission(Manifest.permission.INTERNET,member)==PackageManager.PERMISSION_GRANTED;
            mutable&=!protectedPackage(member);s.meteredMutable&=meteredMutable(member,dpm,roles);
        }
        s.identity=identity.toString();s.policy=manager.getUidPolicy(app.uid);s.dataSaver=manager.getRestrictBackground();s.lineage=lineage();s.availability="available";
        s.writable=internet&&mutable&&!users.hasUserRestriction("no_control_apps")&&!users.hasUserRestriction("no_config_mobile_networks");
        return s;
    }
    @Override public void write(AppNetworkBackend.State observed,Field field,boolean enabled)throws Exception{
        NetworkPolicyManager manager=context.getSystemService(NetworkPolicyManager.class);
        if(manager==null)throw new IllegalStateException("Network policy unavailable");
        // Add/remove only our finite bits. Never replace the whole UID mask.
        int bit=field==Field.BACKGROUND?1:field==Field.UNRESTRICTED?4:field==Field.NETWORK?0x40000:field==Field.WIFI?0x8000:field==Field.MOBILE?0x10000:0x20000;
        boolean add=field==Field.UNRESTRICTED?enabled:!enabled;
        if(add)manager.addUidPolicy(observed.uid,bit);else manager.removeUidPolicy(observed.uid,bit);
        if(field==Field.BACKGROUND)manager.removeUidPolicy(observed.uid,4);
        if(field==Field.UNRESTRICTED)manager.removeUidPolicy(observed.uid,1);
    }
}
