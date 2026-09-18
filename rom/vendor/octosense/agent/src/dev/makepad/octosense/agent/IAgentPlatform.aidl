package dev.makepad.octosense.agent;

import android.content.Intent;
import android.os.Bundle;

/**
 * The agent platform: privileged powers behind one Binder surface. Every call
 * checks the caller's signature and package (see AgentPlatformService.Caller).
 *
 * Results are Bundles with an "ok" boolean and, on failure, a "reason" code the
 * launcher's result_copy turns into a sentence: "denied", "keyguard",
 * "unavailable", "failed".
 */
interface IAgentPlatform {
    /** Protocol version and the capability set present on this device. */
    Bundle getCapabilities();

    /** tasks: "tasks" = list of Bundles {id, package, activity, label, visible, lastActive}. */
    Bundle getTasks(int max);
    /** tasks: "png" = the task's last snapshot as PNG, or reason "unavailable". */
    Bundle getTaskSnapshot(int taskId, int maxWidth);

    /** screen: "png" = the primary display now, scaled to at most maxWidth px wide; "width", "height". */
    Bundle captureScreen(int maxWidth);

    /** input: refused while the keyguard is showing. */
    Bundle tap(float x, float y);
    Bundle swipe(float x0, float y0, float x1, float y1, int durationMs);
    Bundle typeText(String text);
    Bundle pressKey(int keyCode);

    /** settings: secure/system/global by table name. */
    Bundle getSetting(String table, String name);
    Bundle putSetting(String table, String name, String value);

    /** apps */
    Bundle startActivity(in Intent intent);
    Bundle startTask(int taskId);
    Bundle removeTask(int taskId);
    Bundle forceStop(String packageName);

    /** statusbar */
    Bundle expandNotifications();
    Bundle expandQuickSettings();
    Bundle collapsePanels();

    /** The last calls, newest first: who, what, outcome. */
    Bundle getAuditLog(int max);
}
