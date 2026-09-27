package dev.makepad.octosense.agent;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.os.Bundle;

/** The one notification the updater owns: available, progress, restart. */
final class UpdateNotifier {
    static final String CHANNEL = "updates";
    static final int ID = 7001;
    private final Context context;
    private final NotificationManager nm;

    UpdateNotifier(Context context) {
        this.context = context;
        nm = context.getSystemService(NotificationManager.class);
        nm.createNotificationChannel(new NotificationChannel(CHANNEL, "OctoSense updates",
                NotificationManager.IMPORTANCE_DEFAULT));
    }

    private PendingIntent action(String action) {
        Intent i = new Intent(context, UpdateActionReceiver.class).setAction(action);
        return PendingIntent.getBroadcast(context, action.hashCode(), i, PendingIntent.FLAG_IMMUTABLE);
    }

    private Notification.Builder base(String title, String text) {
        return new Notification.Builder(context, CHANNEL)
                .setSmallIcon(android.R.drawable.stat_sys_download_done)
                .setContentTitle(title).setContentText(text).setOnlyAlertOnce(true);
    }

    void available(Bundle check) {
        StringBuilder what = new StringBuilder();
        if (check.getBoolean("rom_newer")) what.append("System ").append(check.getString("rom_offered"));
        if (check.getBoolean("home_newer")) {
            if (what.length() > 0) what.append(" and ");
            what.append("Home app");
        }
        nm.notify(ID, base("OctoSense update available", what.toString())
                .addAction(new Notification.Action.Builder(null, "Install", action(UpdateActionReceiver.APPLY)).build())
                .build());
    }

    void progress(Bundle status) {
        String phase = status.getString("rom_phase", "idle");
        if ("updated_need_reboot".equals(phase)) {
            nm.notify(ID, base("OctoSense update installed", "Restart to finish")
                    .addAction(new Notification.Action.Builder(null, "Restart", action(UpdateActionReceiver.REBOOT)).build())
                    .build());
        } else if ("downloading".equals(phase) || "verifying".equals(phase) || "finalizing".equals(phase)) {
            int pct = Math.round(status.getFloat("rom_progress") * 100);
            nm.notify(ID, base("Installing OctoSense update", phase + " " + pct + "%")
                    .setProgress(100, pct, false).setOngoing(true).build());
        } else if ("failed".equals(phase)) {
            nm.notify(ID, base("OctoSense update failed", status.getString("rom_error", ""))
                    .addAction(new Notification.Action.Builder(null, "Retry", action(UpdateActionReceiver.APPLY)).build())
                    .build());
        }
    }
}
