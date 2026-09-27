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
import dev.makepad.octosense.applanguage.AppLanguageBackend;
import dev.makepad.octosense.applanguage.AppLanguageContract;
import java.util.Arrays;

public final class OctoSenseAppLanguageService extends Service {
    private AppLanguageBackend backend;
    @Override public void onCreate(){super.onCreate();backend=new AppLanguageBackend(new AppLanguagePlatform(this),SystemClock::elapsedRealtime);}
    private void caller(){int uid=Binder.getCallingUid();String[] packages=getPackageManager().getPackagesForUid(uid);if(UserHandle.getUserId(uid)!=0||UserHandle.myUserId()!=0||packages==null||!Arrays.asList(packages).contains("dev.makepad.octosense.agent")||getPackageManager().checkSignatures(uid,Process.myUid())!=PackageManager.SIGNATURE_MATCH)throw new SecurityException("Caller is not the trusted Settings helper");}
    private final IAppLanguageSettings.Stub binder=new IAppLanguageSettings.Stub(){
        @Override public Bundle snapshot(long id,String pkg,String key,String parent,String query,int offset){caller();AppLanguageContract.read(id,pkg,key,parent,query,offset);long identity=Binder.clearCallingIdentity();try{Bundle out=new Bundle();out.putString("json",backend.snapshot(id,pkg,key,parent,query,offset).toString());out.putBoolean("ok",true);return out;}catch(Exception|LinkageError unknown){Bundle out=new Bundle();out.putBoolean("ok",false);return out;}finally{Binder.restoreCallingIdentity(identity);}}
        @Override public String select(String pkg,String key,String choice){caller();AppLanguageContract.packageName(pkg);AppLanguageContract.key(key);AppLanguageContract.key(choice);long identity=Binder.clearCallingIdentity();try{return backend.select(pkg,key,choice);}catch(Exception|LinkageError unknown){return "app_language_unconfirmed";}finally{Binder.restoreCallingIdentity(identity);}}
    };
    @Override public IBinder onBind(Intent intent){return binder;}
    @Override public void onDestroy(){if(backend!=null)backend.invalidate();super.onDestroy();}
}
