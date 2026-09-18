package dev.makepad.octosense.bridge.validation;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.pm.PackageManager;
import android.os.Bundle;
import android.os.IBinder;
import dev.makepad.octosense.contracts.ISystemBridge;
import dev.makepad.octosense.contracts.IHomeIntegration;
import dev.makepad.octosense.contracts.Protocol;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/** A same-signer, same-user caller outside the Home package must be rejected. */
public final class CallerRejectionInstrumentation extends Instrumentation {
    private final CountDownLatch connected=new CountDownLatch(1);
    private volatile ISystemBridge bridge;
    private volatile IHomeIntegration home;
    private boolean quickstep;
    private final ServiceConnection connection=new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name,IBinder binder) {
            if(quickstep) home=IHomeIntegration.Stub.asInterface(binder);
            else bridge=ISystemBridge.Stub.asInterface(binder);
            connected.countDown();
        }
        @Override public void onServiceDisconnected(ComponentName name) {bridge=null;home=null;}
    };
    @Override public void onCreate(Bundle args) {quickstep=args!=null&&"quickstep".equals(args.getString("target"));start();}
    @Override public void onStart() {
        Bundle report=new Bundle();boolean bound=false;
        try {
            Context context=getTargetContext();
            if(!Protocol.BRIDGE_PACKAGE.equals(context.getPackageName())) throw new AssertionError("wrong_target_package");
            if(context.getPackageManager().checkSignatures(Protocol.BRIDGE_PACKAGE,Protocol.HOME_PACKAGE)
                    !=PackageManager.SIGNATURE_MATCH) throw new AssertionError("signer_precondition_failed");
            report.putInt("caller_uid",android.os.Process.myUid());
            String target=quickstep?Protocol.QUICKSTEP_PACKAGE:Protocol.BRIDGE_PACKAGE;
            if(quickstep&&context.checkSelfPermission(Protocol.HOME_BIND_PERMISSION)!=PackageManager.PERMISSION_GRANTED)
                throw new AssertionError("signature_permission_precondition_failed");
            report.putString("target_package",target);
            bound=context.bindService(new Intent().setComponent(new ComponentName(target,
                    target+(quickstep?".HomeIntegrationService":".SystemBridgeService"))),connection,Context.BIND_AUTO_CREATE);
            if(!bound||!connected.await(10,TimeUnit.SECONDS)||(quickstep?home==null:bridge==null)) throw new AssertionError("bind_timeout");
            try {
                if(quickstep) home.getProtocolInfo();else bridge.getProtocolInfo();
                throw new AssertionError("foreign_package_was_accepted");
            } catch(SecurityException expected) {
                report.putBoolean("same_signer_foreign_package_rejected",true);
                report.putString("rejection",expected.getMessage());
            }
            report.putString("result","pass");
        } catch(Throwable e) {report.putString("result","fail");report.putString("failure",e.toString());}
        finally {
            if(bound) getTargetContext().unbindService(connection);
            finish("pass".equals(report.getString("result"))?Activity.RESULT_OK:Activity.RESULT_CANCELED,report);
        }
    }
}
