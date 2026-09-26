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
import dev.makepad.octosense.systemlanguage.SystemLanguageContract;
import dev.makepad.octosense.systemlanguage.SystemLanguageSettings;
import java.util.Arrays;

public final class OctoSenseSystemLanguageService extends Service {
    private SystemLanguageSettings backend;
    @Override public void onCreate(){super.onCreate();backend=new SystemLanguageSettings(new SystemLanguagePlatform(this),SystemClock::elapsedRealtime);}
    private void caller(){int uid=Binder.getCallingUid();String[] packages=getPackageManager().getPackagesForUid(uid);if(UserHandle.getUserId(uid)!=0||UserHandle.myUserId()!=0||packages==null||!Arrays.asList(packages).contains("dev.makepad.octosense.agent")||getPackageManager().checkSignatures(uid,Process.myUid())!=PackageManager.SIGNATURE_MATCH)throw new SecurityException("Caller is not the trusted Settings helper");}
    private final ISystemLanguageSettings.Stub binder=new ISystemLanguageSettings.Stub(){
        @Override public Bundle snapshot(long id,String key,String parent,String query,int offset){caller();SystemLanguageContract.read(id,key,parent,query,offset);long identity=Binder.clearCallingIdentity();try{Bundle result=new Bundle();result.putBoolean("ok",true);result.putString("json",backend.snapshot(id,key,parent,query,offset).toString());return result;}catch(Exception|LinkageError unknown){Bundle result=new Bundle();result.putBoolean("ok",false);return result;}finally{Binder.restoreCallingIdentity(identity);}}
        @Override public String apply(String key,String[] targets){caller();SystemLanguageContract.order(key,targets);long identity=Binder.clearCallingIdentity();try{return backend.apply(key,targets);}catch(Exception|LinkageError unknown){return "languages_unconfirmed";}finally{Binder.restoreCallingIdentity(identity);}}
    };
    @Override public IBinder onBind(Intent intent){return binder;}
    @Override public void onDestroy(){if(backend!=null)backend.invalidate();super.onDestroy();}
}
