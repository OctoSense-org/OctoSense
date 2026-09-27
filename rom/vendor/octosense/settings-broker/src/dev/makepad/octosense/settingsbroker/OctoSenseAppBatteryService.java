package dev.makepad.octosense.settingsbroker;

import android.app.Service;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.os.Binder;
import android.os.Bundle;
import android.os.IBinder;
import android.os.Process;
import android.os.SystemClock;
import android.os.UserHandle;
import dev.makepad.octosense.battery.AppBatteryBackend;
import dev.makepad.octosense.battery.AppBatteryContract;
import java.util.Arrays;

/** The exact trusted helper can use finite owner-only battery policy operations. */
public final class OctoSenseAppBatteryService extends Service {
    private AppBatteryBackend backend;
    @Override public void onCreate(){super.onCreate();backend=new AppBatteryBackend(new AppBatteryPlatform(this),SystemClock::elapsedRealtime);}
    private void caller(){int uid=Binder.getCallingUid();String[] packages=getPackageManager().getPackagesForUid(uid);
        if(UserHandle.getUserId(uid)!=0||UserHandle.myUserId()!=0||packages==null
                ||!Arrays.asList(packages).contains("dev.makepad.octosense.agent")
                ||getPackageManager().checkSignatures(uid,Process.myUid())!=PackageManager.SIGNATURE_MATCH)
            throw new SecurityException("Caller is not the trusted Settings helper");}
    private final IAppBatterySettings.Stub binder=new IAppBatterySettings.Stub(){
        @Override public Bundle snapshot(long id,String pkg){caller();AppBatteryContract.packageName(pkg);long identity=Binder.clearCallingIdentity();
            try{Bundle result=new Bundle();result.putString("json",backend.snapshot(id,pkg).toString());result.putBoolean("ok",true);return result;}
            catch(Exception|LinkageError unavailable){Bundle result=new Bundle();result.putBoolean("ok",false);return result;}
            finally{Binder.restoreCallingIdentity(identity);}}
        @Override public String set(String pkg,String key,String mode){caller();AppBatteryContract.packageName(pkg);AppBatteryContract.key(key);AppBatteryContract.Mode.parse(mode);long identity=Binder.clearCallingIdentity();
            try{return backend.set(pkg,key,mode);}catch(Exception|LinkageError unavailable){return "app_battery_unconfirmed";}finally{Binder.restoreCallingIdentity(identity);}}
    };
    @Override public IBinder onBind(Intent intent){return binder;}
}
