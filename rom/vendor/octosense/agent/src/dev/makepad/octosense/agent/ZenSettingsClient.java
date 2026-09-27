package dev.makepad.octosense.agent;

import android.content.ComponentName;
import android.content.Context;
import android.os.Bundle;
import android.os.RemoteException;
import dev.makepad.octosense.settingsbroker.IZenSettings;

/** Optional system-owned manual DND adapter. Failed binds are retried by visible-page reads. */
final class ZenSettingsClient implements AutoCloseable {
    private final SettingsServiceConnection<IZenSettings> connection;
    ZenSettingsClient(Context context) {
        connection = new SettingsServiceConnection<>(context, new ComponentName(
                "dev.makepad.octosense.settingsbroker", "dev.makepad.octosense.settingsbroker.OctoSenseZenSettingsService"), IZenSettings.Stub::asInterface);
    }
    Bundle snapshot() {
        IZenSettings current=connection.current();
        if(current==null) {return null;}
        try {Bundle result=current.snapshot();return result!=null&&result.getBoolean("ok")?result:null;}
        catch(RemoteException|SecurityException unavailable) {return null;}
    }
    boolean apply(String mode) {
        dev.makepad.octosense.controls.DndMode.parse(mode);
        IZenSettings current=connection.current();if(current==null) return false;
        try {return current.setMode(mode);}
        catch(RemoteException|SecurityException unavailable) {return false;}
    }
    private interface Operation {Bundle run(IZenSettings service) throws RemoteException;}
    private Bundle request(Operation operation) {
        IZenSettings current=connection.current();
        if(current==null){return null;}
        try{return operation.run(current);}catch(RemoteException|SecurityException unavailable){return null;}
    }
    Bundle settingsSnapshot(long id,int offset,String generation) {return request(service->service.settingsSnapshot(id,offset,generation));}
    Bundle policy(String key,String field,String value) {return request(service->service.policy(key,field,value));}
    Bundle schedule(String key,String target,String name,int[] days,int start,int end,boolean exitAtAlarm,boolean enabled) {return request(service->service.schedule(key,target,name,days,start,end,exitAtAlarm,enabled));}
    Bundle enabled(String key,String target,boolean enabled) {return request(service->service.enabled(key,target,enabled));}
    Bundle deleteRule(String key,String target) {return request(service->service.deleteRule(key,target));}
    @Override public void close() {connection.close();}
}
