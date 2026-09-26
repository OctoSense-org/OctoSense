package dev.makepad.octosense;

/** The Settings app's finite vocabulary; never accepts an intent, component or user ID. */
public final class AppsSettingsContract {
    private AppsSettingsContract() {}
    public static final int PAGE_SIZE=20, MAX_OFFSET=100000;
    public enum Action {
        LAUNCH("launch"), UNINSTALL("uninstall"), APP_INFO("app_info"), NOTIFICATIONS("notifications"), LANGUAGE("language");
        public final String wire;
        Action(String wire) {this.wire=wire;}
        public static Action parse(String wire) {
            for(Action action:values()) if(action.wire.equals(wire)) return action;
            throw new IllegalArgumentException("Unknown app action");
        }
    }
    public static String packageName(String value) {
        if(value==null||value.length()>255||(!value.equals("android")&&!value.contains("."))
                ||!value.matches("[A-Za-z_][A-Za-z0-9_]*(\\.[A-Za-z_][A-Za-z0-9_]*)*"))
            throw new IllegalArgumentException("Invalid package name");
        return value;
    }
    public static String query(String value) {
        if(value==null||value.codePointCount(0,value.length())>128||value.codePoints().anyMatch(Character::isISOControl))
            throw new IllegalArgumentException("Invalid app filter");
        return value;
    }
    public static int offset(Object value) {
        if(!(value instanceof Integer||value instanceof Long)) throw new IllegalArgumentException("Integer page offset required");
        long offset=((Number)value).longValue();
        if(offset<0||offset>MAX_OFFSET||offset%PAGE_SIZE!=0) throw new IllegalArgumentException("Invalid page offset");
        return (int)offset;
    }
    public static String generation(String value) {
        if(value!=null&&!value.matches("[0-9a-f]{64}")) throw new IllegalArgumentException("Invalid catalog generation");
        return value;
    }
    public static boolean protectedPackage(String value) {
        return value.equals("android")||value.equals("com.android.settings")||value.startsWith("dev.makepad.octosense");
    }
}
