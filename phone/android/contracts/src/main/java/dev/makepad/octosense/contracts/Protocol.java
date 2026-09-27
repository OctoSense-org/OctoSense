package dev.makepad.octosense.contracts;

import android.os.Bundle;

/** Stable wire constants. Discovery and validation are different capabilities. */
public final class Protocol {
    public static final int MAJOR = 1;
    public static final int MINOR = 1;
    public static final String HOME_PACKAGE = "dev.makepad.octosense";
    public static final String BRIDGE_PACKAGE = HOME_PACKAGE + ".bridge";
    public static final String QUICKSTEP_PACKAGE = HOME_PACKAGE + ".quickstep";
    public static final String BIND_PERMISSION = HOME_PACKAGE + ".permission.BIND_SYSTEM_BRIDGE";
    public static final String HOME_BIND_PERMISSION = HOME_PACKAGE + ".permission.BIND_HOME_INTEGRATION";
    public static final int MAX_PACKET_BYTES = 256 * 1024;
    public static final int MAX_NOTIFICATIONS = 100;
    public static final int MAX_ACTIONS = 8;
    public static final int ACCEPTED = 0;
    public static final int COMPLETED = 1;
    public static final int ACCESS_DENIED = 2;
    public static final int UNSUPPORTED = 3;
    public static final int PREREQUISITE_MISSING = 4;
    public static final int EXPIRED_HANDLE = 5;
    public static final int INCOMPATIBLE = 6;
    public static final int DISCONNECTED = 7;
    public static final int TIMEOUT = 8;
    public static final int UNCERTAIN = 9;
    public static final int INVALID_ARGUMENT = 10;
    public static final int QUEUE_FULL = 11;

    private Protocol() {}

    public static Bundle info(String implementation) {
        Bundle result = new Bundle();
        result.putInt("major", MAJOR);
        result.putInt("minor", MINOR);
        result.putString("implementation", implementation);
        return result;
    }

    public static void requireSession(String session) {
        if (session == null || session.length() < 8 || session.length() > 128)
            throw new IllegalArgumentException("Invalid session identity");
    }

    public static void requireCommand(long commandId) {
        if (commandId <= 0) throw new IllegalArgumentException("Invalid command identity");
    }

    public static void requireUnitValue(float value) {
        if (Float.isNaN(value) || Float.isInfinite(value) || value < 0 || value > 1)
            throw new IllegalArgumentException("Value must be finite and between zero and one");
    }
}
