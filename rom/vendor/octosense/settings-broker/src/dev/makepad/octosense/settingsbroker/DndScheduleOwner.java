package dev.makepad.octosense.settingsbroker;

/** Both verified Android spellings of its built-in time-condition provider. */
final class DndScheduleOwner {
    private DndScheduleOwner() {}

    static boolean matches(String rulePackage, String ownerPackage, String ownerClass) {
        return "android".equals(rulePackage) && "android".equals(ownerPackage)
                && ("ScheduleConditionProvider".equals(ownerClass)
                    || "com.android.server.notification.ScheduleConditionProvider".equals(ownerClass));
    }
}
