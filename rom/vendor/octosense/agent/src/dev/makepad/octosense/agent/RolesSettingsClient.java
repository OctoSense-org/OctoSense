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
import com.android.permissioncontroller.octosense.IRoleSettings;
import dev.makepad.octosense.roles.RolesSettingsContract;
import dev.makepad.octosense.roles.RolesSettingsContract.RoleId;

/** Native role qualification and confirmation remain in PermissionController. */
final class RolesSettingsClient implements AutoCloseable {
    private final Context context;
    private final Handler main=new Handler(Looper.getMainLooper());
    private volatile IRoleSettings service;
    private boolean bound,closed;
    private long lastAttempt;
    RolesSettingsClient(Context context){this.context=context;main.post(this::bind);}
    private void bind(){
        if(bound||closed||SystemClock.elapsedRealtime()-lastAttempt<5000)return;
        lastAttempt=SystemClock.elapsedRealtime();
        if(context.getPackageManager().checkSignatures(context.getPackageName(),"com.android.permissioncontroller")!=PackageManager.SIGNATURE_MATCH)return;
        try{bound=context.bindService(new Intent().setComponent(new ComponentName("com.android.permissioncontroller",
                "com.android.permissioncontroller.octosense.OctoSenseRolesService")),connection,Context.BIND_AUTO_CREATE);}
        catch(SecurityException unavailable){bound=false;}
    }
    private final ServiceConnection connection=new ServiceConnection(){
        @Override public void onServiceConnected(ComponentName name,IBinder binder){service=IRoleSettings.Stub.asInterface(binder);}
        @Override public void onServiceDisconnected(ComponentName name){service=null;}
        @Override public void onBindingDied(ComponentName name){unbind();}
        @Override public void onNullBinding(ComponentName name){unbind();}
    };
    private void unbind(){if(bound)context.unbindService(connection);bound=false;service=null;}
    Bundle snapshot(long id,String role,int offset,String generation)throws RemoteException{
        RoleId selected=role==null?null:RoleId.parse(role);RolesSettingsContract.page(id,selected,offset,generation);
        IRoleSettings current=service;if(current==null){main.post(this::bind);return null;}
        return current.snapshot(id,role,offset,generation);
    }
    Bundle confirmation(String role,String key,String target)throws RemoteException{
        RoleId.parse(role);RolesSettingsContract.key(key);RolesSettingsContract.key(target);
        IRoleSettings current=service;return current==null?null:current.confirmation(role,key,target);
    }
    @Override public void close(){closed=true;main.removeCallbacksAndMessages(null);unbind();}
}
