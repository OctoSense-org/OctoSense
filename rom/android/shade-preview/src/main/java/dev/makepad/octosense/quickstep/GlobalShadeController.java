package dev.makepad.octosense.quickstep;

import android.os.Bundle;
import dev.makepad.octosense.contracts.Protocol;
import java.util.ArrayList;

/** Isolated fixture host; production controller is never compiled into this app. */
final class GlobalShadeController {
    static final java.util.concurrent.CopyOnWriteArraySet<Runnable> observers = new java.util.concurrent.CopyOnWriteArraySet<>();
    static android.content.SharedPreferences preferences(android.content.Context context) {
        android.content.SharedPreferences p=context.getSharedPreferences("design-fixture",0);
        if(!p.contains("enabled")) p.edit().putBoolean("enabled",true).apply();
        return p;
    }
    static String status() { return "Active · Pull down from either top corner in any app"; }
    final Bundle state = new Bundle();
    GlobalShadePanel panel;
    String destination;
    boolean closed;
    GlobalShadeController(String scenario) {
        state.putString("network_summary", "Wi-Fi · Connected");
        state.putBoolean("wifi", true); state.putBoolean("bluetooth", true);
        state.putBoolean("rotation_locked", true);
        state.putFloat("brightness", .64f); state.putFloat("volume", .28f);
        state.putBoolean("brightness_automatic", scenario.equals("automatic"));
        Bundle capabilities = new Bundle();
        for (String operation : new String[]{"notifications", "brightness", "volume", "torch", "rotation", "wifi", "bluetooth"}) {
            Bundle capability = new Bundle();
            capability.putBoolean("accessible", !scenario.equals("permissions") && !operation.equals("wifi") && !operation.equals("bluetooth"));
            capabilities.putBundle(operation, capability);
        }
        state.putBundle("capabilities", capabilities);
        ArrayList<Bundle> notices = new ArrayList<>();
        if (!scenario.equals("empty")) {
            notices.add(notice("Messages", "Alex Morgan", "The photos look great. Coffee tomorrow at 10?", true, 2));
            notices.add(notice("Calendar", "Design review", "10:30–11:00 · Studio room", true, 12));
            notices.add(notice("Downloads", "Offline playlist", "12 of 18 tracks downloaded", false, 25));
        }
        state.putParcelableArrayList("notifications", notices);
    }
    private Bundle notice(String app, String title, String text, boolean clear, int minutes) {
        Bundle b = new Bundle(); b.putString("app_label", app); b.putString("package", "dev.makepad.octosense.shadepreview");
        b.putString("identity", app); b.putString("handle", app); b.putString("title", title); b.putString("text", text);
        b.putLong("posted", System.currentTimeMillis() - minutes * 60000L); b.putBoolean("dismissible", clear);
        ArrayList<Bundle> actions = new ArrayList<>();
        if (app.equals("Messages")) {
            Bundle reply = new Bundle(); reply.putString("handle", "fixture-reply"); reply.putString("label", "Reply"); reply.putBoolean("reply", true); actions.add(reply);
        }
        b.putParcelableArrayList("actions", actions); return b;
    }
    boolean accessible(String operation) {
        Bundle b = state.getBundle("capabilities").getBundle(operation);
        return b != null && b.getBoolean("accessible");
    }
    void dismiss() { closed = true; }
    void settings(String destination) { this.destination = destination; }
    long command(String operation, boolean toggle, float value, String handle, String reply) {
        if (operation.equals("brightness") || operation.equals("volume")) state.putFloat(operation, value);
        else if (operation.equals("rotation")) state.putBoolean("rotation_locked", toggle);
        else if (operation.equals("torch")) state.putBoolean("torch", toggle);
        panel.post(() -> { panel.update(state); panel.result(1, Protocol.COMPLETED); });
        return 1;
    }
}
