package dev.makepad.octosense.settingsbroker;

import android.app.ActivityManager;
import android.app.KeyguardManager;
import android.app.NotificationManager;
import android.app.Service;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.os.Binder;
import android.os.Bundle;
import android.os.IBinder;
import android.os.Process;
import android.os.UserHandle;
import android.os.UserManager;
import dev.makepad.octosense.controls.DndMode;
import java.util.Arrays;

/** The system-owned manual mode API, accessible only through the trusted helper. */
public final class OctoSenseZenSettingsService extends Service {
    private dev.makepad.octosense.dnd.DndSettingsBackend settings;
    @Override public void onCreate() {
        super.onCreate();settings=new dev.makepad.octosense.dnd.DndSettingsBackend(new DndPlatformSettings(this),android.os.SystemClock::elapsedRealtime);
    }
    private interface SettingsOperation {Bundle run() throws Exception;}
    private Bundle settings(SettingsOperation operation) {
        caller();long identity=Binder.clearCallingIdentity();
        try {return operation.run();}
        catch(Exception | LinkageError unavailable) {Bundle result=new Bundle();result.putBoolean("ok",false);result.putString("reason","dnd_unavailable");return result;}
        finally {Binder.restoreCallingIdentity(identity);}
    }
    private static Bundle result(String reason) {Bundle result=new Bundle();result.putBoolean("ok",reason.equals("dnd_applied")||reason.equals("dnd_requested"));result.putString("reason",reason);return result;}
    private void caller() {
        int uid=Binder.getCallingUid();String[] packages=getPackageManager().getPackagesForUid(uid);
        if(UserHandle.getUserId(uid)!=UserHandle.myUserId()||packages==null
                ||!Arrays.asList(packages).contains("dev.makepad.octosense.agent")
                ||getPackageManager().checkSignatures(uid,Process.myUid())!=PackageManager.SIGNATURE_MATCH)
            throw new SecurityException("Caller is not the trusted Settings helper");
    }
    private boolean unlockedOwner() {
        KeyguardManager lock=getSystemService(KeyguardManager.class);
        UserManager users=getSystemService(UserManager.class);
        return Process.myUid()==Process.SYSTEM_UID&&ActivityManager.getCurrentUser()==UserHandle.USER_SYSTEM
                &&UserHandle.myUserId()==UserHandle.USER_SYSTEM&&users!=null&&users.isUserUnlocked()
                &&lock!=null&&!lock.isKeyguardLocked();
    }
    private boolean writable() {
        UserManager users=getSystemService(UserManager.class);
        return unlockedOwner()&&users.isAdminUser()&&!users.hasUserRestriction(UserManager.DISALLOW_ADJUST_VOLUME);
    }
    private Bundle state() {
        Bundle result=new Bundle();boolean ok=unlockedOwner();result.putBoolean("ok",ok);
        if(!ok) return result;
        NotificationManager manager=getSystemService(NotificationManager.class);
        DndMode mode=manager==null?null:DndMode.fromZen(manager.getZenMode());
        if(mode!=null) result.putString("mode",mode.wire);
        result.putBoolean("writable",mode!=null&&writable());
        return result;
    }
    private final IZenSettings.Stub binder=new IZenSettings.Stub() {
        @Override public Bundle settingsSnapshot(long id,int offset,String generation) {return settings(()->{
            Bundle result=new Bundle();result.putBoolean("ok",true);result.putString("json",settings.snapshot(id,offset,generation).toString());return result;
        });}
        @Override public Bundle policy(String key,String field,String value) {return settings(()->result(settings.policy(key,dev.makepad.octosense.dnd.DndSettingsContract.Field.parse(field),value)));}
        @Override public Bundle schedule(String key,String target,String name,int[] days,int start,int end,boolean exitAtAlarm,boolean enabled) {return settings(()->result(settings.schedule(key,target,new dev.makepad.octosense.dnd.DndSettingsContract.Schedule(name,days,start,end,exitAtAlarm,enabled))));}
        @Override public Bundle enabled(String key,String target,boolean value) {return settings(()->result(settings.enabled(key,target,value)));}
        @Override public Bundle deleteRule(String key,String target) {return settings(()->result(settings.delete(key,target)));}
        @Override public Bundle snapshot() {
            caller();long token=Binder.clearCallingIdentity();
            try {return state();} finally {Binder.restoreCallingIdentity(token);}
        }
        @Override public synchronized boolean setMode(String value) {
            caller();DndMode mode=DndMode.parse(value);long token=Binder.clearCallingIdentity();
            try {
                NotificationManager manager=getSystemService(NotificationManager.class);
                if(!writable()||manager==null||DndMode.fromZen(manager.getZenMode())==null) return false;
                // The same user-origin route as Android Settings. Public app APIs
                // on Android 15 may create an implicit rule instead of changing
                // the system's manual mode. Null condition means until changed.
                manager.setZenMode(mode.zen,null,"OctoSenseSettings",true);
                return true; // Acceptance only; the helper independently reads back.
            } finally {Binder.restoreCallingIdentity(token);}
        }
    };
    @Override public IBinder onBind(Intent intent) {return binder;}
}
