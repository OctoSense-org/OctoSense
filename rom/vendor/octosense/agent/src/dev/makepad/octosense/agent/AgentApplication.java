package dev.makepad.octosense.agent;

import android.app.Application;
import android.content.Intent;

/**
 * The process is persistent, so it is up from boot; starting the service here
 * keeps its Binder alive without a client, which is what the dumpsys harness
 * (scripts/agent-test.sh) and the audit log rely on.
 */
public class AgentApplication extends Application {
    @Override public void onCreate() {
        super.onCreate();
        startService(new Intent(this, AgentPlatformService.class));
    }
}
