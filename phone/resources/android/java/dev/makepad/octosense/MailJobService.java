package dev.makepad.octosense;

import android.app.job.JobParameters;
import android.app.job.JobService;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import org.json.JSONObject;

/** Network-constrained, bounded, quiet work. Never starts an Activity or FGS. */
public final class MailJobService extends JobService {
    private final Handler main = new Handler(Looper.getMainLooper());
    private Run current;
    private static final class Run {
        volatile boolean cancelled;
        long lease;
    }
    @Override public boolean onStartJob(JobParameters params) {
        if (current != null) return false;
        Run run = new Run(); current = run;
        new Thread(() -> {
            long started = SystemClock.elapsedRealtime();
            long collectedAfter = System.currentTimeMillis()/1000;
            android.util.Log.i("OctoMail", "Background mail job started");
            try {
                if (!MailBackground.init(getApplicationContext())) return;
                synchronized (run) {
                    if (run.cancelled) return;
                    run.lease = MailBackground.nativeBegin();
                }
                int ticks = 0;
                while (!run.cancelled && SystemClock.elapsedRealtime()-started < 240000) {
                    boolean snapshot = ticks++ % 10 == 0;
                    JSONObject state = new JSONObject(MailBackground.nativeTick(snapshot));
                    if (snapshot && !state.has("error")) {
                        MailBackground.post(getApplicationContext(), state);
                        if (!state.optBoolean("enabled")) {
                            MailBackground.schedule(getApplicationContext(), false);
                            break;
                        }
                        if (!state.isNull("pending") && state.optInt("pending", -1) == 0
                            && state.optLong("last_poll_at") >= collectedAfter) break;
                    }
                    Thread.sleep(100);
                }
            } catch (Exception error) {
                android.util.Log.w("OctoMail", "Background mail job incomplete; durable events retained");
            } finally {
                synchronized (run) { if (run.lease != 0) MailBackground.nativeEnd(run.lease); }
                android.util.Log.i("OctoMail", "Background mail job ended");
                main.post(() -> {
                    if (current == run) { current = null; if (!run.cancelled) jobFinished(params, false); }
                });
            }
        }, "mail-job").start();
        return true;
    }
    private void stopRun() {
        Run run = current; current = null;
        if (run != null) synchronized (run) {
            run.cancelled = true;
            if (run.lease != 0) MailBackground.nativeEnd(run.lease);
        }
    }
    @Override public boolean onStopJob(JobParameters params) {
        stopRun();
        return false; // Keep the ordinary periodic schedule; no aggressive retry loop.
    }
    @Override public void onDestroy() { stopRun(); super.onDestroy(); }
}
