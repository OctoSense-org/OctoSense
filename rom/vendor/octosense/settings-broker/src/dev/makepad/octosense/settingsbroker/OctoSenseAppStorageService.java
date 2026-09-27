package dev.makepad.octosense.settingsbroker;

import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.os.Binder;
import android.os.Bundle;
import android.os.IBinder;
import android.os.Process;
import android.os.SystemClock;
import android.os.UserHandle;
import dev.makepad.octosense.appstorage.AppStorageBackend;
import dev.makepad.octosense.appstorage.AppStorageContract;
import java.util.Arrays;

public final class OctoSenseAppStorageService extends Service {
    private AppStorageBackend backend;
    @Override public void onCreate(){super.onCreate();backend=new AppStorageBackend(new AppStoragePlatform(this),SystemClock::elapsedRealtime);}
    private void caller(){int uid=Binder.getCallingUid();String[] packages=getPackageManager().getPackagesForUid(uid);if(UserHandle.getUserId(uid)!=0||UserHandle.myUserId()!=0||packages==null||!Arrays.asList(packages).contains("dev.makepad.octosense.agent")||getPackageManager().checkSignatures(uid,Process.myUid())!=PackageManager.SIGNATURE_MATCH)throw new SecurityException("Caller is not the trusted Settings helper");}
    private final IAppStorageSettings.Stub binder=new IAppStorageSettings.Stub(){
        @Override public Bundle snapshot(long id,String pkg){caller();AppStorageContract.packageName(pkg);long identity=Binder.clearCallingIdentity();try{Bundle out=new Bundle();out.putString("json",backend.snapshot(id,pkg).toString());out.putBoolean("ok",true);return out;}catch(Exception|LinkageError unknown){Bundle out=new Bundle();out.putBoolean("ok",false);return out;}finally{Binder.restoreCallingIdentity(identity);}}
        @Override public Bundle action(String pkg,String key,String action){caller();AppStorageContract.packageName(pkg);AppStorageContract.key(key);AppStorageContract.Action.parse(action);long identity=Binder.clearCallingIdentity();try{AppStorageBackend.Result result=backend.action(pkg,key,action);Bundle out=new Bundle();out.putString("reason",result.reason);out.putBoolean("ok",true);if(result.nativeFlow instanceof PendingIntent)out.putParcelable("flow",(PendingIntent)result.nativeFlow);return out;}catch(Exception|LinkageError unknown){Bundle out=new Bundle();out.putString("reason","app_storage_unconfirmed");out.putBoolean("ok",false);return out;}finally{Binder.restoreCallingIdentity(identity);}}
    };
    @Override public IBinder onBind(Intent intent){return binder;}
}
