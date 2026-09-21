package dev.makepad.octosense.agent;

import android.app.Activity;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.pm.PackageManager;
import android.os.Bundle;
import android.os.IBinder;
import android.os.RemoteException;
import android.util.Log;

import java.util.ArrayList;
import java.util.List;
import java.util.function.Consumer;

/**
 * The launcher's binding to the OctoSense ROM's agent platform. Present only
 * on the ROM (the package is platform-signed and absent elsewhere), so every
 * caller keeps its Home-app fallback: this client reports "absent" and the
 * shell carries on with UsageStats and the bridge.
 */
public final class AgentPlatformClient {
    public static final String PACKAGE = "dev.makepad.octosense.agent";
    static final String TAG = "OctoSenseAgentClient";

    /** One recent task, as the platform lists it. */
    public static final class Task {
        public final int id; public final String pkg; public final String label; public final boolean visible; public final long lastActive;
        Task(int id, String pkg, String label, boolean visible, long lastActive) {
            this.id = id; this.pkg = pkg; this.label = label; this.visible = visible; this.lastActive = lastActive;
        }
    }

    private final Activity activity;
    private final Consumer<String> onState;
    private IAgentPlatform platform;
    private boolean bound;
    private List<String> capabilities = new ArrayList<>();

    public AgentPlatformClient(Activity activity, Consumer<String> onState) {
        this.activity = activity; this.onState = onState;
    }

    /** True when the ROM ships the platform and it is signed like us. */
    public boolean available() {
        PackageManager pm = activity.getPackageManager();
        try {
            pm.getPackageInfo(PACKAGE, 0);
        } catch (PackageManager.NameNotFoundException e) {
            return false;
        }
        return pm.checkSignatures(activity.getPackageName(), PACKAGE) == PackageManager.SIGNATURE_MATCH;
    }

    public boolean connected() { return platform != null; }
    public boolean has(String capability) { return platform != null && capabilities.contains(capability); }

    public void bind() {
        if (bound || !available()) { if (!bound) onState.accept("absent"); return; }
        try {
            bound = activity.bindService(new Intent("dev.makepad.octosense.agent.AGENT_PLATFORM")
                    .setComponent(new ComponentName(PACKAGE, PACKAGE + ".AgentPlatformService")),
                    connection, Context.BIND_AUTO_CREATE);
            if (!bound) onState.accept("bind_failed");
        } catch (SecurityException e) {
            onState.accept("denied");
        }
    }

    public void unbind() {
        if (bound) { activity.unbindService(connection); bound = false; }
        platform = null;
    }

    private final ServiceConnection connection = new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name, IBinder binder) {
            IAgentPlatform candidate = IAgentPlatform.Stub.asInterface(binder);
            try {
                Bundle caps = candidate.getCapabilities();
                ArrayList<String> list = caps.getStringArrayList("capabilities");
                capabilities = list == null ? new ArrayList<>() : list;
                platform = candidate;
                onState.accept("connected");
            } catch (RemoteException | SecurityException e) {
                Log.w(TAG, "handshake failed", e);
                platform = null;
                onState.accept("handshake_failed");
            }
        }
        @Override public void onServiceDisconnected(ComponentName name) { platform = null; onState.accept("disconnected"); }
        @Override public void onNullBinding(ComponentName name) { platform = null; onState.accept("absent"); }
    };

    /** Recent tasks, most recent first, the launcher's own excluded; empty when the capability is absent. */
    public List<Task> recentTasks(int max) {
        ArrayList<Task> out = new ArrayList<>();
        if (!has("tasks")) return out;
        try {
            Bundle r = platform.getTasks(max + 1);
            @SuppressWarnings("deprecation")
            ArrayList<Bundle> tasks = r.getParcelableArrayList("tasks");
            if (!r.getBoolean("ok") || tasks == null) return out;
            for (Bundle t : tasks) {
                String pkg = t.getString("package");
                if (pkg == null || pkg.equals(activity.getPackageName())) continue;
                out.add(new Task(t.getInt("id"), pkg, t.getString("label", pkg), t.getBoolean("visible"), t.getLong("lastActive")));
                if (out.size() >= max) break;
            }
        } catch (RemoteException | SecurityException e) {
            Log.w(TAG, "tasks failed", e);
        }
        return out;
    }

    /** Brings a task forward; false when the platform refused or is absent. */
    public boolean startTask(int taskId) {
        if (!has("apps")) return false;
        try { return platform.startTask(taskId).getBoolean("ok"); }
        catch (RemoteException | SecurityException e) { return false; }
    }

    /** Opens the system shade through the platform; false when absent. */
    public boolean expandNotifications() {
        if (!has("statusbar")) return false;
        try { return platform.expandNotifications().getBoolean("ok"); }
        catch (RemoteException | SecurityException e) { return false; }
    }
}
