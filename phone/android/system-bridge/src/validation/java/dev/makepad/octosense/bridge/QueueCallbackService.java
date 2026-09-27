package dev.makepad.octosense.bridge;

import android.app.Service;
import android.content.Intent;
import android.os.Binder;
import android.os.Bundle;
import android.os.IBinder;
import android.os.Parcel;
import android.os.Process;
import android.os.RemoteException;
import dev.makepad.octosense.contracts.ISystemBridgeCallback;

/** Validation-only callback process. It can terminate only itself, for a real
 * Binder death test; no renderer, device setter, or external caller is involved. */
public final class QueueCallbackService extends Service {
    static final int IDENTITY=IBinder.FIRST_CALL_TRANSACTION+100;
    static final int EXIT=IDENTITY+1;
    static final String CONTROL="dev.makepad.octosense.bridge.QueueCallbackControl";
    private final ISystemBridgeCallback.Stub callback=new ISystemBridgeCallback.Stub() {
        @Override public void onSnapshot(String epoch,long revision,Bundle state) {}
        @Override public void onDelta(String epoch,long revision,Bundle patch) {}
        @Override public void onResyncRequired(String epoch) {}
        @Override public void onCommandResult(String session,long id,int status,String reason) {}
        @Override public boolean onTransact(int code,Parcel data,Parcel reply,int flags) throws RemoteException {
            if(code==IDENTITY || code==EXIT) {
                if(Binder.getCallingUid()!=Process.myUid()) throw new SecurityException("Wrong validation UID");
                data.enforceInterface(CONTROL);
                if(code==IDENTITY) {reply.writeNoException();reply.writeInt(Process.myPid());}
                else Process.killProcess(Process.myPid());
                return true;
            }
            return super.onTransact(code,data,reply,flags);
        }
    };
    @Override public IBinder onBind(Intent intent) {return callback;}
}
