package dev.makepad.octosense.bridge;

import android.content.Intent;
import android.os.Build;
import android.os.Bundle;
import android.os.IBinder;
import com.topjohnwu.superuser.ipc.RootService;
import dev.makepad.octosense.contracts.CallerGuard;
import dev.makepad.octosense.contracts.Protocol;
import java.io.IOException;
import java.util.concurrent.Semaphore;
import java.util.concurrent.TimeUnit;

/** Fixed target-ROM adapter. Root is a capability, never a general exec API. */
public final class ScopedRootService extends RootService {
    private final Semaphore operation = new Semaphore(1);
    private String lineageDevice="";
    private String lineageVersion="";
    @Override public void onCreate() {
        lineageDevice=property("ro.lineage.device");
        lineageVersion=property("ro.lineage.version");
    }
    private void caller() { CallerGuard.require(this, Protocol.BRIDGE_PACKAGE); }
    @Override public IBinder onBind(Intent intent) { return binder; }
    private final IRootControl.Stub binder = new IRootControl.Stub() {
        @Override public Bundle getIdentity() {
            caller();
            Bundle value = Protocol.info("lineage-22.2-enchilada/1");
            value.putInt("uid", android.os.Process.myUid());
            value.putString("fingerprint", Build.FINGERPRINT);
            value.putString("lineage_device",lineageDevice);
            value.putString("lineage_version",lineageVersion);
            value.putBoolean("adapter_supported", supported());
            // The caller still has to verify enforcing-mode behavior on the phone.
            value.putBoolean("setters_validated", false);
            return value;
        }
        @Override public Bundle setWifiEnabled(boolean enabled) {
            caller(); return execute("wifi", "set-wifi-enabled", enabled ? "enabled" : "disabled");
        }
        @Override public Bundle setBluetoothEnabled(boolean enabled) {
            caller(); return execute("bluetooth_manager", enabled ? "enable" : "disable");
        }
        @Override public Bundle setBatterySaver(boolean enabled) {
            caller(); return execute("power", "set-mode", enabled ? "1" : "0");
        }
    };
    private boolean supported() {
        return android.os.Process.myUid() == 0 && Build.VERSION.SDK_INT == 35
                && "enchilada".equals(lineageDevice) && lineageVersion.startsWith("22.2-");
    }
    private static String property(String key) {
        java.lang.Process child=null;
        try {
            child=new ProcessBuilder("/system/bin/getprop",key).start();
            if(!child.waitFor(2,TimeUnit.SECONDS) || child.exitValue()!=0) return "";
            byte[] bytes=new byte[1024];int count=child.getInputStream().read(bytes);
            return count>0 ? new String(bytes,0,count,java.nio.charset.StandardCharsets.UTF_8).trim() : "";
        } catch(IOException|InterruptedException e) { return ""; }
        finally { if(child!=null) child.destroy(); }
    }
    private Bundle execute(String... arguments) {
        if (!supported()) return result(Protocol.UNSUPPORTED, "rom_adapter_unavailable");
        if (!operation.tryAcquire()) return result(Protocol.QUEUE_FULL, "root_operation_busy");
        java.lang.Process child = null;
        try {
            String[] argv = new String[arguments.length + 1];
            argv[0] = "/system/bin/cmd";
            System.arraycopy(arguments, 0, argv, 1, arguments.length);
            child = new ProcessBuilder(argv).redirectErrorStream(true).start();
            if (!child.waitFor(5, TimeUnit.SECONDS)) {
                child.destroyForcibly();
                return result(Protocol.UNCERTAIN, "root_command_timeout_reconcile_state");
            }
            return child.exitValue() == 0 ? result(Protocol.COMPLETED, "request_dispatched_reconcile_state")
                    : result(Protocol.ACCESS_DENIED, "rom_command_rejected");
        } catch (IOException e) {
            return result(Protocol.UNSUPPORTED, "rom_command_unavailable");
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            return result(Protocol.UNCERTAIN, "root_command_interrupted_reconcile_state");
        } finally {
            if (child != null) child.destroy();
            operation.release();
        }
    }
    static Bundle result(int status, String reason) {
        Bundle result = new Bundle(); result.putInt("status", status); result.putString("reason", reason); return result;
    }
}
