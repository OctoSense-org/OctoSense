package dev.makepad.octosense;

/** Finite Settings vocabulary shared by the local and ROM-service adapters. */
public enum DeviceSetting {
    SCREEN_TIMEOUT("screen_timeout_ms", "system", "screen_off_timeout"),
    FONT_SCALE("font_scale", "system", "font_scale"),
    TOUCH_SOUNDS("touch_sounds", "system", "sound_effects_enabled"),
    HAPTIC_FEEDBACK("haptic_feedback", "system", "haptic_feedback_enabled"),
    HOUR_FORMAT("hour_format", "system", "time_12_24"),
    AUTO_TIME("auto_time", "global", "auto_time"),
    AUTO_TIME_ZONE("auto_time_zone", "global", "auto_time_zone"),
    VOLUME_MEDIA("volume_media", "", ""),
    VOLUME_ALARM("volume_alarm", "", ""),
    VOLUME_RING("volume_ring", "", ""),
    VOLUME_NOTIFICATION("volume_notification", "", "");

    public final String wire, table, key;
    DeviceSetting(String wire, String table, String key) { this.wire=wire;this.table=table;this.key=key; }
    public boolean audio() { return table.isEmpty(); }
    public static DeviceSetting parse(String wire) {
        for(DeviceSetting setting:values()) if(setting.wire.equals(wire)) return setting;
        throw new IllegalArgumentException("Unknown setting");
    }
    public void validate(Object value) {
        if(this==SCREEN_TIMEOUT) {
            if(!(value instanceof Integer || value instanceof Long)) throw new IllegalArgumentException("Integer timeout required");
            long timeout=((Number)value).longValue();
            for(long allowed:new long[]{15000,30000,60000,120000,300000,600000}) if(timeout==allowed) return;
        } else if(this==FONT_SCALE) {
            if(!(value instanceof Number)) throw new IllegalArgumentException("Numeric scale required");
            double scale=((Number)value).doubleValue();
            for(double allowed:new double[]{.85,1,1.15,1.3,1.5}) if(Double.isFinite(scale)&&Math.abs(scale-allowed)<.00001) return;
        } else if(audio()) {
            if(value instanceof Number) {
                double volume=((Number)value).doubleValue();
                if(Double.isFinite(volume)&&volume>=0&&volume<=1) return;
            }
        } else if(value instanceof Boolean) return;
        throw new IllegalArgumentException("Invalid setting value");
    }
    public String storedValue(Object value) {
        validate(value);
        if(audio()) throw new IllegalArgumentException("Audio is controlled by AudioManager");
        if(value instanceof Boolean) return this==HOUR_FORMAT ? ((Boolean)value?"24":"12") : ((Boolean)value?"1":"0");
        return value.toString();
    }
}
