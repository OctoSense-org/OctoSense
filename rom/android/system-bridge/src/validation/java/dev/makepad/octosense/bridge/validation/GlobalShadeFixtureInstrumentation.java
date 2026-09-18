package dev.makepad.octosense.bridge.validation;

import android.app.Instrumentation;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.RemoteInput;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.graphics.drawable.Icon;
import android.os.Bundle;
import android.os.SystemClock;

/** Local notification fixture only. Registered exclusively in the validation APK. */
public final class GlobalShadeFixtureInstrumentation extends Instrumentation {
    private static final String COMMAND = "dev.makepad.octosense.bridge.GLOBAL_SHADE_FIXTURE";
    private String token, channel, title, replyAction;
    private Context context;
    private NotificationManager notifications;
    private PendingIntent replyTarget, openTarget;
    private volatile boolean finished;
    private volatile int replies;
    private volatile String received = "";
    private boolean cleanupOnly;
    @Override public void onCreate(Bundle arguments) {
        token = arguments == null ? "" : arguments.getString("token", "");
        if (!token.matches("[a-f0-9]{32}")) throw new IllegalArgumentException("Fixture token required");
        cleanupOnly = "true".equals(arguments.getString("cleanup_only"));
        start();
    }
    private void post(boolean update) {
        notifications.notify(token, 71, new Notification.Builder(context, channel)
                .setSmallIcon(android.R.drawable.ic_dialog_info).setContentTitle(title)
                .setContentText(update ? "Fixture updated" : "Fixture delivery")
                .setOnlyAlertOnce(true).setContentIntent(openTarget)
                .addAction(new Notification.Action.Builder(Icon.createWithResource(context, android.R.drawable.ic_menu_send), "Reply", replyTarget)
                        .addRemoteInput(new RemoteInput.Builder("reply").setLabel("Reply to fixture").build()).build()).build());
        notifications.notify(token, 72, new Notification.Builder(context, channel)
                .setSmallIcon(android.R.drawable.ic_dialog_info).setContentTitle(title + " ongoing")
                .setContentText("Owned ongoing fixture").setOngoing(true).setOnlyAlertOnce(true).build());
    }
    private void status(String phase) {
        boolean posted = false;
        for (android.service.notification.StatusBarNotification n : notifications.getActiveNotifications())
            if (token.equals(n.getTag()) && n.getId() == 71) posted = true;
        Bundle value = new Bundle(); value.putString("phase", phase); value.putString("fixture_title", title);
        value.putBoolean("posted", posted); value.putInt("replies", replies); value.putString("reply", received);
        sendStatus(1, value);
    }
    private final BroadcastReceiver commands = new BroadcastReceiver() {
        @Override public void onReceive(Context c, Intent intent) {
            if (!token.equals(intent.getStringExtra("token"))) return;
            String operation = intent.getStringExtra("operation");
            if ("post".equals(operation)) post(false);
            else if ("update".equals(operation)) post(true);
            else if ("finish".equals(operation)) finished = true;
            status(operation == null ? "state" : operation);
        }
    };
    private final BroadcastReceiver replyReceiver = new BroadcastReceiver() {
        @Override public void onReceive(Context c, Intent intent) {
            Bundle input = RemoteInput.getResultsFromIntent(intent);
            received = input == null ? "" : String.valueOf(input.getCharSequence("reply"));
            replies++; status("reply_received");
        }
    };
    @Override public void onStart() {
        context = getTargetContext(); notifications = context.getSystemService(NotificationManager.class);
        // Recover only this fixture's private namespace after a lost ADB watcher.
        for (android.service.notification.StatusBarNotification item : notifications.getActiveNotifications()) {
            String tag = item.getTag();
            if (tag != null && tag.matches("[a-f0-9]{32}") && (item.getId() == 71 || item.getId() == 72)
                    && ("global-shade-" + tag).equals(item.getNotification().getChannelId())) notifications.cancel(tag, item.getId());
        }
        for (NotificationChannel old : notifications.getNotificationChannels())
            if (old.getId().matches("global-shade-[a-f0-9]{32}")) notifications.deleteNotificationChannel(old.getId());
        if (cleanupOnly) { Bundle result = new Bundle(); result.putString("result", "pass"); finish(-1, result); return; }
        title = "OctoSense global fixture " + token.substring(0, 6); channel = "global-shade-" + token;
        replyAction = COMMAND + ".REPLY." + token;
        Bundle result = new Bundle();
        try {
            if (android.os.Build.VERSION.SDK_INT < 33) throw new IllegalStateException("Native panel fixture requires Android 13 or later");
            if (!notifications.areNotificationsEnabled()) throw new AssertionError("Fixture post permission missing");
            NotificationChannel nc = new NotificationChannel(channel, "OctoSense panel test", NotificationManager.IMPORTANCE_LOW);
            nc.setSound(null, null); notifications.createNotificationChannel(nc);
            context.registerReceiver(commands, new IntentFilter(COMMAND), android.Manifest.permission.DUMP, null, Context.RECEIVER_EXPORTED);
            context.registerReceiver(replyReceiver, new IntentFilter(replyAction), Context.RECEIVER_NOT_EXPORTED);
            replyTarget = PendingIntent.getBroadcast(context, 71, new Intent(replyAction).setPackage(context.getPackageName()), PendingIntent.FLAG_MUTABLE | PendingIntent.FLAG_CANCEL_CURRENT);
            openTarget = PendingIntent.getActivity(context, 72, new Intent().setClassName(context.getPackageName(), context.getPackageName() + ".BridgeSettingsActivity"), PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_CANCEL_CURRENT);
            status("ready");
            long deadline = SystemClock.elapsedRealtime() + 600_000;
            while (!finished && SystemClock.elapsedRealtime() < deadline) SystemClock.sleep(100);
            if (!finished || replies != 1 || !"OctoSense_global_reply".equals(received)) throw new AssertionError("Fixture flow incomplete");
            result.putString("result", "pass"); result.putInt("replies", replies); result.putBoolean("external_message_sent", false);
        } catch (Throwable failure) { result.putString("result", "fail"); result.putString("failure", failure.toString()); }
        finally {
            notifications.cancel(token, 71); notifications.cancel(token, 72); notifications.deleteNotificationChannel(channel);
            if (replyTarget != null) replyTarget.cancel(); if (openTarget != null) openTarget.cancel();
            try { context.unregisterReceiver(commands); } catch (IllegalArgumentException ignored) {}
            try { context.unregisterReceiver(replyReceiver); } catch (IllegalArgumentException ignored) {}
        }
        finish("pass".equals(result.getString("result")) ? -1 : 0, result);
    }
}
