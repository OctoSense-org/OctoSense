package dev.makepad.octosense.display;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/** Reviewed display operations; neither density numbers nor system keys are arbitrary inputs. */
public final class DisplaySettingsContract {
    private DisplaySettingsContract() {}
    public enum Setting {
        DENSITY("density"), ACTIVATED("activated"), TEMPERATURE("temperature"),
        MODE("mode"), START("start"), END("end");
        public final String wire;
        Setting(String wire) { this.wire = wire; }
        public static Setting parse(String wire) {
            for (Setting setting : values()) if (setting.wire.equals(wire)) return setting;
            throw new IllegalArgumentException("Unknown display setting");
        }
    }
    public static String key(String value) {
        if (value == null || !value.matches("[0-9a-f]{64}")) throw new IllegalArgumentException("Invalid display target");
        return value;
    }
    public static boolean recent(long at, long now) { return at >= 0 && now >= at && now - at <= 20000; }
    public static int integer(String value, int min, int max) {
        if (value == null || !value.matches("0|[1-9][0-9]{0,5}")) throw new IllegalArgumentException("Integer required");
        int number = Integer.parseInt(value);
        if (number < min || number > max) throw new IllegalArgumentException("Display value out of range");
        return number;
    }
    public static String value(Setting setting, String value) {
        switch (setting) {
            case DENSITY: return key(value);
            case ACTIVATED:
                if (!"on".equals(value) && !"off".equals(value)) throw new IllegalArgumentException("On or off required");
                break;
            case TEMPERATURE: integer(value, 1, 100000); break;
            case START: case END: integer(value, 0, 86399); break;
            case MODE:
                if (!"disabled".equals(value) && !"custom".equals(value) && !"sunset".equals(value))
                    throw new IllegalArgumentException("Invalid Night Light schedule");
                break;
        }
        return value;
    }
    /** SDK/JSON boundary: reject coercion of strings, fractions and null to another type. */
    public static String encode(Setting setting, Object value) {
        if (setting == Setting.ACTIVATED) {
            if (!(value instanceof Boolean)) throw new IllegalArgumentException("Boolean required");
            return (Boolean)value ? "on" : "off";
        }
        if (setting == Setting.TEMPERATURE || setting == Setting.START || setting == Setting.END) {
            if (!(value instanceof Integer) && !(value instanceof Long)) throw new IllegalArgumentException("Integer required");
            return value(setting, value.toString());
        }
        if (!(value instanceof String)) throw new IllegalArgumentException("String required");
        return value(setting, (String)value);
    }
    public static String fingerprint(String... fields) {
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            for (String field : fields) {
                byte[] bytes = field.getBytes(StandardCharsets.UTF_8);
                digest.update(ByteBuffer.allocate(4).putInt(bytes.length).array()); digest.update(bytes);
            }
            StringBuilder key = new StringBuilder();
            for (byte b : digest.digest()) key.append(String.format(Locale.ROOT, "%02x", b & 255));
            return key.toString();
        } catch (java.security.NoSuchAlgorithmException impossible) { throw new IllegalStateException(impossible); }
    }
    /**
     * Pinned SettingsLib density choices: 70–150%, minimum 9% steps, at most
     * three sizes on either side; larger choices leave at least 320 logical
     * pixels on the short edge. Only these observed choices can be written. Current custom values
     * are reported separately, never inserted into this write allowlist.
     */
    public static List<Integer> densities(int initialDpi, int shortEdgePixels) {
        ArrayList<Integer> values = new ArrayList<>();
        if (initialDpi <= 0 || initialDpi > 10000 || shortEdgePixels < 320 || shortEdgePixels > 32768) return values;
        float maximum = Math.min(1.5f, (160 * shortEdgePixels / 320) / (float)initialDpi);
        int smaller = Math.min(3, (int)((1.0f - 0.7f) / 0.09f));
        int larger = Math.max(0, Math.min(3, (int)((maximum - 1.0f) / 0.09f)));
        float smallerStep = (1.0f - 0.7f) / smaller;
        for (int i = smaller; i > 0; i--) {
            int dpi = ((int)(initialDpi * (1.0f - i * smallerStep))) & ~1;
            if (dpi > 0 && !values.contains(dpi)) values.add(dpi);
        }
        values.add(initialDpi);
        if (larger > 0) {
            float largerStep = (maximum - 1.0f) / larger;
            for (int i = 1; i <= larger; i++) {
                int dpi = ((int)(initialDpi * (1.0f + i * largerStep))) & ~1;
                if (dpi > 0 && !values.contains(dpi)) values.add(dpi);
            }
        }
        return values;
    }
}
