package dev.makepad.octosense;

import android.Manifest;
import android.app.Activity;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.job.JobInfo;
import android.app.job.JobScheduler;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.os.Build;
import android.service.notification.StatusBarNotification;
import org.json.JSONArray;
import org.json.JSONObject;
import java.util.HashSet;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.TimeUnit;

/** Same account and Rust host as Home; no Activity is needed by the job. */
public final class MailBackground {
    static final int JOB_ID = 0x4d41494c;
    static final String OPEN = "dev.makepad.octosense.MAIL_CARD";
    static final String TOKEN = "mail_card_token";
    private static final String CHANNEL = "important_mail";
    private static final String GLANCE_CHANNEL = "app_cards";
    private static final String TAG = "OctoMail.";
    private static final ScheduledExecutorService monitor = Executors.newSingleThreadScheduledExecutor(r -> {
        Thread t = new Thread(r, "mail-notification-monitor"); t.setDaemon(true); return t;
    });
    private static ScheduledFuture<?> foregroundMonitor;
    private static boolean permissionAsked;
    static { System.loadLibrary("makepad"); }
    private MailBackground() {}
    static native boolean nativeInit(String files, String kernel);
    static native void nativeForeground(boolean active);
    static native long nativeBegin();
    static native void nativeEnd(long lease);
    static native String nativeTick(boolean snapshot);
    static native void nativeDelivered(String token, long published);

    static boolean init(Context context) {
        return nativeInit(context.getFilesDir().getAbsolutePath(),
            context.getApplicationInfo().nativeLibraryDir + "/liboctos.so");
    }
    public static synchronized void resume(Activity activity) {
        nativeForeground(true);
        if (foregroundMonitor != null) foregroundMonitor.cancel(false);
        Context context = activity.getApplicationContext();
        foregroundMonitor = monitor.scheduleWithFixedDelay(() -> {
            try {
                if (!init(context)) return;
                JSONObject state = new JSONObject(nativeTick(true));
                if (state.has("error")) return;
                boolean enabled = state.optBoolean("enabled");
                schedule(context, enabled);
                post(context, state);
                boolean hasNotices = state.optJSONArray("notifications") != null
                    && state.optJSONArray("notifications").length() > 0;
                if ((enabled || hasNotices) && Build.VERSION.SDK_INT >= 33 && !permissionAsked
                    && !context.getSharedPreferences("mail-background", Context.MODE_PRIVATE).getBoolean("notification-permission-asked", false)
                    && context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
                    permissionAsked = true;
                    activity.runOnUiThread(() -> {
                        if (!activity.isFinishing() && activity.hasWindowFocus()) {
                            context.getSharedPreferences("mail-background", Context.MODE_PRIVATE).edit()
                                .putBoolean("notification-permission-asked", true).apply();
                            activity.requestPermissions(new String[]{Manifest.permission.POST_NOTIFICATIONS}, 0x4d41);
                        }
                    });
                }
            } catch (Exception error) { android.util.Log.w("OctoMail", "Foreground mail state unavailable"); }
        }, 1, 2, TimeUnit.SECONDS);
    }
    public static synchronized void pause() {
        nativeForeground(false);
        if (foregroundMonitor != null) { foregroundMonitor.cancel(false); foregroundMonitor = null; }
    }
    static synchronized void schedule(Context context, boolean enabled) {
        JobScheduler jobs = context.getSystemService(JobScheduler.class);
        if (jobs == null) return;
        if (!enabled) { jobs.cancel(JOB_ID); return; }
        if (jobs.getPendingJob(JOB_ID) != null) return; // Do not reset the OS's due time on every resume.
        JobInfo job = new JobInfo.Builder(JOB_ID, new ComponentName(context, MailJobService.class))
            .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
            .setPeriodic(15 * 60 * 1000L, 5 * 60 * 1000L)
            .setPersisted(true).build();
        if (jobs.schedule(job) != JobScheduler.RESULT_SUCCESS)
            android.util.Log.w("OctoMail", "Periodic mail job could not be scheduled");
    }
    /** Immutable navigation-only PendingIntent; private content on the lock screen. */
    static synchronized void post(Context context, JSONObject state) throws Exception {
        NotificationManager manager = context.getSystemService(NotificationManager.class);
        if (manager == null) return;
        boolean chinese = context.getResources().getConfiguration().getLocales().get(0).getLanguage().equals("zh");
        NotificationChannel channel = new NotificationChannel(CHANNEL,
            chinese ? "重要邮件" : "Important mail", NotificationManager.IMPORTANCE_DEFAULT);
        channel.setLockscreenVisibility(Notification.VISIBILITY_PRIVATE);
        manager.createNotificationChannel(channel);
        NotificationChannel cards = new NotificationChannel(GLANCE_CHANNEL,
            chinese ? "应用卡片" : "App cards", NotificationManager.IMPORTANCE_DEFAULT);
        cards.setLockscreenVisibility(Notification.VISIBILITY_PRIVATE);
        manager.createNotificationChannel(cards);
        HashSet<String> active = new HashSet<>();
        JSONArray current = state.optJSONArray("active");
        if (current != null) for (int i=0; i<current.length(); i++) active.add(TAG + current.getString(i));
        for (StatusBarNotification old : manager.getActiveNotifications())
            if (old.getTag() != null && old.getTag().startsWith(TAG) && !active.contains(old.getTag()))
                manager.cancel(old.getTag(), 1);
        if (!manager.areNotificationsEnabled()) return;
        JSONArray notices = state.optJSONArray("notifications");
        if (notices == null) return;
        for (int i=0; i<notices.length(); i++) {
            JSONObject notice = notices.getJSONObject(i);
            boolean mail = "mail".equals(notice.optString("kind", "mail"));
            String channelId = mail ? CHANNEL : GLANCE_CHANNEL;
            if (manager.getNotificationChannel(channelId).getImportance() == NotificationManager.IMPORTANCE_NONE) continue;
            String label = mail ? "OctoSense Mail" : "OctoSense";
            int icon = mail ? android.R.drawable.ic_dialog_email : android.R.drawable.ic_dialog_info;
            String token = notice.getString("token");
            if (!token.matches("[0-9a-f]{64}")) continue;
            Intent intent = context.getPackageManager().getLaunchIntentForPackage(context.getPackageName());
            if (intent == null) continue;
            intent.setAction(OPEN).putExtra(TOKEN, token)
                .setData(android.net.Uri.parse("octosense-mail://card/" + token))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_SINGLE_TOP);
            PendingIntent tap = PendingIntent.getActivity(context, 0, intent,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
            Notification publicVersion = new Notification.Builder(context, channelId)
                .setSmallIcon(icon).setContentTitle(label).build();
            Notification notification = new Notification.Builder(context, channelId)
                .setSmallIcon(icon)
                .setContentTitle(notice.optString("title", label))
                .setContentText(notice.optString("summary"))
                .setStyle(new Notification.BigTextStyle().bigText(notice.optString("summary")))
                .setContentIntent(tap).setAutoCancel(true).setOnlyAlertOnce(true)
                .setVisibility(Notification.VISIBILITY_PRIVATE).setPublicVersion(publicVersion)
                .setCategory(mail ? Notification.CATEGORY_EMAIL : Notification.CATEGORY_STATUS)
                .setTimeoutAfter(Math.max(1, notice.getLong("expires") - System.currentTimeMillis())).build();
            manager.notify(TAG + token, 1, notification);
            nativeDelivered(token, notice.getLong("published"));
        }
    }
    public static String entry(Intent intent) {
        try {
            if (intent == null || !OPEN.equals(intent.getAction()) || intent.getSelector() != null) return null;
            String token = intent.getStringExtra(TOKEN);
            return token != null && token.matches("[0-9a-f]{64}") ? token : null;
        } catch (RuntimeException invalid) { return null; }
    }
}
