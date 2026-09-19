package dev.makepad.octosense.agent;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.os.Bundle;
import android.util.Log;

/** The notification's buttons: install what the last check found, or restart. */
public class UpdateActionReceiver extends BroadcastReceiver {
    static final String APPLY = "dev.makepad.octosense.agent.UPDATE_APPLY";
    static final String REBOOT = "dev.makepad.octosense.agent.UPDATE_REBOOT";

    @Override public void onReceive(Context context, Intent intent) {
        AgentApplication app = AgentApplication.get();
        if (REBOOT.equals(intent.getAction())) {
            app.updater().reboot();
            return;
        }
        if (!APPLY.equals(intent.getAction())) return;
        PendingResult pending = goAsync();
        app.work().execute(() -> {
            try {
                Bundle check = app.updater().check();
                if (check.getBoolean("home_newer")) app.updater().applyHome();
                if (check.getBoolean("rom_newer")) app.updater().applyRom();
            } catch (Exception e) {
                Log.w(Updater.TAG, "apply failed", e);
            } finally {
                pending.finish();
            }
        });
    }
}
