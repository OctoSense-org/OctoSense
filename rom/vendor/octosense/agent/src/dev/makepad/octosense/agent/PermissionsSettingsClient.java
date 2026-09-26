package dev.makepad.octosense.agent;

import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.pm.PackageManager;
import android.os.Bundle;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.RemoteException;
import android.os.SystemClock;
import com.android.permissioncontroller.octosense.IPermissionSettings;
import dev.makepad.octosense.permissions.PermissionsSettingsContract;
import dev.makepad.octosense.permissions.PermissionsSettingsContract.Group;

/** Native runtime choices and operation consent remain in PermissionController. */
final class PermissionsSettingsClient implements AutoCloseable {
    private final Context context;
    private final Handler main=new Handler(Looper.getMainLooper());
    private volatile IPermissionSettings service;
    private boolean bound,closed;
    private long lastAttempt;
    PermissionsSettingsClient(Context context){this.context=context;main.post(this::bind);}
    private void bind(){
        if(bound||closed||SystemClock.elapsedRealtime()-lastAttempt<5000)return;
        lastAttempt=SystemClock.elapsedRealtime();
        if(context.getPackageManager().checkSignatures(context.getPackageName(),"com.android.permissioncontroller")!=PackageManager.SIGNATURE_MATCH)return;
        try{bound=context.bindService(new Intent().setComponent(new ComponentName("com.android.permissioncontroller",
                "com.android.permissioncontroller.octosense.OctoSensePermissionsService")),connection,Context.BIND_AUTO_CREATE);}
        catch(SecurityException unavailable){bound=false;}
    }
    private final ServiceConnection connection=new ServiceConnection(){
        @Override public void onServiceConnected(ComponentName name,IBinder binder){service=IPermissionSettings.Stub.asInterface(binder);}
        @Override public void onServiceDisconnected(ComponentName name){service=null;}
        @Override public void onBindingDied(ComponentName name){unbind();}
        @Override public void onNullBinding(ComponentName name){unbind();}
    };
    private void unbind(){if(bound)context.unbindService(connection);bound=false;service=null;}
    Bundle snapshot(long id,String pkg,String group,int offset,String generation)throws RemoteException{
        Group selected=group==null?null:Group.parse(group);PermissionsSettingsContract.page(id,pkg,selected,offset,generation);
        IPermissionSettings current=service;if(current==null){main.post(this::bind);return null;}
        return current.snapshot(id,pkg,group,offset,generation);
    }
    Bundle operation(String pkg,String group,String key,String target)throws RemoteException{
        PermissionsSettingsContract.packageName(pkg);Group.parse(group);PermissionsSettingsContract.key(key);PermissionsSettingsContract.key(target);
        IPermissionSettings current=service;return current==null?null:current.operation(pkg,group,key,target);
    }
    @Override public void close(){closed=true;main.removeCallbacksAndMessages(null);unbind();}
}
