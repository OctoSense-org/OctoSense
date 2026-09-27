package dev.makepad.octosense.agent;

import android.app.Application;
import android.content.Intent;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * The process is persistent, so it is up from boot; starting the service here
 * keeps its Binder alive without a client, which is what the dumpsys harness
 * (scripts/agent-test.sh) and the audit log rely on. It also owns the one
 * Updater, shared by the Binder surface, the periodic check and the
 * notification actions.
 */
public class AgentApplication extends Application {
    private static AgentApplication instance;
    private Updater updater;
    private UpdateNotifier notifier;
    private UpdateSettings updateSettings;
    private final ExecutorService work = Executors.newSingleThreadExecutor();

    @Override public void onCreate() {
        super.onCreate();
        instance = this;
        updater = new Updater(this);
        notifier = new UpdateNotifier(this);
        updater.setListener(notifier::progress);
        updateSettings=new UpdateSettings(this,updater,work);
        startService(new Intent(this, AgentPlatformService.class));
        UpdateJobService.schedule(this);
    }

    static AgentApplication get() { return instance; }
    Updater updater() { return updater; }
    UpdateSettings updateSettings() {return updateSettings;}
    UpdateNotifier notifier() { return notifier; }
    /** Network and install work runs here, never on the main or a Binder thread for long. */
    ExecutorService work() { return work; }
}
