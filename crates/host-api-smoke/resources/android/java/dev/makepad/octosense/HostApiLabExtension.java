package dev.makepad.octosense;

import android.content.Intent;
import dev.makepad.android.MakepadActivity;
import dev.makepad.android.MakepadNative;

/** Isolated fixture bridge using Home's exact production calendar adapter.
 * Only the permission-status command is expected: the Rust broker refuses
 * all permission requests and writes made by the fixture's background tool. */
public final class HostApiLabExtension implements MakepadActivity.ApplicationExtension {
    private volatile boolean foreground;
    private final DeviceCalendarClient calendar;
    public HostApiLabExtension(MakepadActivity activity) {
        calendar = new DeviceCalendarClient(activity,
            (channel, result) -> MakepadNative.onAndroidIntegrationEvent(channel, result.toString()),
            () -> foreground);
    }
    @Override public void command(String channel, String payload) {
        if ("device_calendar.probe".equals(channel)) {
            MakepadNative.onAndroidIntegrationEvent("device_calendar.ready", "{}");
        } else if ("device_calendar.command".equals(channel)) {
            calendar.command(payload);
        }
    }
    @Override public void onResume() { foreground = true; }
    @Override public void onPause() { foreground = false; }
    @Override public void onIntent(Intent intent) {}
    @Override public void onDestroy() { foreground = false; calendar.close(); }
}
