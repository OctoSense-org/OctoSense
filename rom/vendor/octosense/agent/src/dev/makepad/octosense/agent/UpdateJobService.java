package dev.makepad.octosense.agent;

import android.app.job.JobInfo;
import android.app.job.JobParameters;
import android.app.job.JobScheduler;
import android.app.job.JobService;
import android.content.ComponentName;
import android.content.Context;
import android.os.Bundle;
import android.provider.Settings;
import android.util.Log;

/**
 * Checks GitHub for a newer release every six hours on any network. With
 * `octosense_update_auto` = 1 (secure setting) it installs by itself and only
 * asks for the restart; otherwise it posts "update available".
 */
public class UpdateJobService extends JobService {
    static final int JOB_ID = 7001;
    static final String SETTING_AUTO = "octosense_update_auto";

    static void schedule(Context context) {
        JobScheduler js = context.getSystemService(JobScheduler.class);
        if (js.getPendingJob(JOB_ID) != null) return;
        js.schedule(new JobInfo.Builder(JOB_ID, new ComponentName(context, UpdateJobService.class))
                .setRequiredNetworkType(JobInfo.NETWORK_TYPE_ANY)
                .setPeriodic(6 * 60 * 60 * 1000L)
                .setPersisted(true)
                .build());
    }

    @Override public boolean onStartJob(JobParameters params) {
        AgentApplication app = AgentApplication.get();
        app.work().execute(() -> {
            try {
                Bundle check = app.updater().check();
                boolean newer = check.getBoolean("rom_newer") || check.getBoolean("home_newer");
                if (newer) {
                    boolean auto = Settings.Secure.getInt(getContentResolver(), SETTING_AUTO, 0) == 1;
                    if (auto) {
                        if (check.getBoolean("home_newer")) app.updater().applyHome();
                        if (check.getBoolean("rom_newer")) app.updater().applyRom();
                    } else {
                        app.notifier().available(check);
                    }
                }
            } catch (Exception e) {
                Log.w(Updater.TAG, "periodic check failed", e);
            }
            jobFinished(params, false);
        });
        return true;
    }

    @Override public boolean onStopJob(JobParameters params) { return true; }
}
