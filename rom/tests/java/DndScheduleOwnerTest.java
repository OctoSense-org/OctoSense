package dev.makepad.octosense.settingsbroker;

public final class DndScheduleOwnerTest {
    private static void check(boolean expected, String pkg, String owner, String name) {
        if (DndScheduleOwner.matches(pkg, owner, name) != expected)
            throw new AssertionError("Unexpected native schedule ownership decision");
    }
    public static void main(String[] args) {
        for (String name : new String[]{"ScheduleConditionProvider",
                "com.android.server.notification.ScheduleConditionProvider"}) {
            check(true, "android", "android", name);
            check(false, "third.party", "android", name);
            check(false, "android", "third.party", name);
            check(false, null, "android", name);
        }
        for (String name : new String[]{null, "", ".ScheduleConditionProvider",
                "EventConditionProvider", "third.party.ScheduleConditionProvider",
                "com.android.server.notification.ScheduleConditionProvider$Other"}) {
            check(false, "android", "android", name);
        }
        System.out.println("Native schedule owner aliases and foreign-provider denial PASS");
    }
}
