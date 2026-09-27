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
import dev.makepad.octosense.appnetwork.AppNetworkBackend;
import dev.makepad.octosense.appnetwork.AppNetworkContract;
import java.util.Arrays;

public final class OctoSenseAppNetworkService extends Service {
    private AppNetworkBackend settings;
    @Override public void onCreate(){super.onCreate();settings=new AppNetworkBackend(new AppNetworkPlatform(this),SystemClock::elapsedRealtime);}
    private interface Operation{Bundle run()throws Exception;}
    private Bundle call(Operation operation){
        int uid=Binder.getCallingUid();String[] packages=getPackageManager().getPackagesForUid(uid);
        if(Process.myUid()!=Process.SYSTEM_UID||UserHandle.getUserId(uid)!=0||packages==null
            ||!Arrays.asList(packages).contains("dev.makepad.octosense.agent")
            ||getPackageManager().checkSignatures(uid,Process.myUid())!=PackageManager.SIGNATURE_MATCH)
            throw new SecurityException("Caller is not the trusted Settings helper");
        long identity=Binder.clearCallingIdentity();
        try{return operation.run();}
        catch(Exception|LinkageError unavailable){return result("app_network_unavailable");}
        finally{Binder.restoreCallingIdentity(identity);}
    }
    private static Bundle result(String reason){Bundle out=new Bundle();out.putBoolean("ok",reason.equals("app_network_applied")||reason.equals("app_network_unchanged"));out.putString("reason",reason);return out;}
    private final IAppNetworkSettings.Stub binder=new IAppNetworkSettings.Stub(){
        @Override public Bundle snapshot(long id,String pkg){return call(()->{
            Bundle out=new Bundle();out.putBoolean("ok",true);out.putString("json",settings.snapshot(id,pkg).toString());return out;
        });}
        @Override public Bundle set(String pkg,String key,String field,boolean enabled){return call(()->result(settings.set(pkg,key,AppNetworkContract.Field.parse(field),enabled)));}
    };
    @Override public IBinder onBind(Intent intent){return binder;}
}
