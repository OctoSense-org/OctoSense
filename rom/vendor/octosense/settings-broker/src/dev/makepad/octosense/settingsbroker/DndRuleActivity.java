package dev.makepad.octosense.settingsbroker;

import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;

/** The two verified native ZenRule status APIs; unsupported status stays unknown. */
final class DndRuleActivity {
    private DndRuleActivity() {}

    static Boolean read(Object rule) {
        if (rule == null) return null;
        Method method;
        try {
            try {
                // Pinned Lineage platform: includes native Modes override handling.
                method = rule.getClass().getMethod("isActive");
            } catch (NoSuchMethodException olderPlatform) {
                // Original AOSP API35: enabled / snoozing / native condition handling.
                method = rule.getClass().getMethod("isAutomaticActive");
            }
            if (method.getReturnType() != boolean.class) return null;
            return (Boolean) method.invoke(rule);
        } catch (InvocationTargetException failure) {
            Throwable cause = failure.getCause();
            // Missing framework linkage is an unsupported observation. VM failures
            // must not be hidden behind an "unavailable" Settings label.
            if (cause instanceof Error && !(cause instanceof LinkageError)) throw (Error) cause;
            return null;
        } catch (ReflectiveOperationException | SecurityException | LinkageError unavailable) {
            return null;
        }
    }
}
