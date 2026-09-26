package dev.makepad.octosense.settingsbroker;

import android.app.Service;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.os.Binder;
import android.os.Bundle;
import android.os.IBinder;
import android.os.Process;
import android.os.UserHandle;
import dev.makepad.octosense.notifications.AppNotificationsContract.Action;
import java.util.Arrays;

/** Bound only by the exact platform-signed helper; each operation rechecks current owner. */
public final class OctoSenseAppNotificationsService extends Service {
    private AppNotificationsBackend backend;
    @Override public void onCreate(){super.onCreate();backend=new AppNotificationsBackend(this);}
    private void caller(){int uid=Binder.getCallingUid();String[] packages=getPackageManager().getPackagesForUid(uid);
        if(UserHandle.getUserId(uid)!=UserHandle.myUserId()||packages==null||!Arrays.asList(packages).contains("dev.makepad.octosense.agent")||getPackageManager().checkSignatures(uid,Process.myUid())!=PackageManager.SIGNATURE_MATCH)throw new SecurityException("Caller is not the trusted Settings helper");}
    private final IAppNotifications.Stub binder=new IAppNotifications.Stub(){
        @Override public Bundle snapshot(long id,String pkg,int offset,String generation){caller();long identity=Binder.clearCallingIdentity();try{Bundle result=new Bundle();result.putString("json",backend.snapshot(id,pkg,offset,generation).toString());result.putBoolean("ok",true);return result;}catch(Exception unavailable){Bundle result=new Bundle();result.putBoolean("ok",false);return result;}finally{Binder.restoreCallingIdentity(identity);}}
        @Override public String set(String pkg,String key,String target,String action,String value){caller();Action parsed=Action.parse(action);long identity=Binder.clearCallingIdentity();try{return backend.apply(pkg,key,target,parsed,value);}catch(Exception unavailable){return "notifications_unconfirmed";}finally{Binder.restoreCallingIdentity(identity);}}
    };
    @Override public IBinder onBind(Intent intent){return binder;}
}
