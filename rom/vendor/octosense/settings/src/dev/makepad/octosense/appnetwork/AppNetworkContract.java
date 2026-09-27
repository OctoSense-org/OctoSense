package dev.makepad.octosense.appnetwork;

/** Finite UID policy vocabulary; no UID or policy mask crosses the UI boundary. */
public final class AppNetworkContract {
    private AppNetworkContract() {}
    public enum Field {
        BACKGROUND("background"), UNRESTRICTED("unrestricted"), NETWORK("network"),
        WIFI("wifi"), MOBILE("mobile"), VPN("vpn");
        public final String wire;
        Field(String wire) { this.wire=wire; }
        public static Field parse(String value) {
            for(Field f:values())if(f.wire.equals(value))return f;
            throw new IllegalArgumentException("Unknown app network field");
        }
    }
    public static String packageName(String value) {
        if("android".equals(value))return value;
        if(value==null||value.length()>255||!value.matches("[A-Za-z_][A-Za-z0-9_]*(\\.[A-Za-z_][A-Za-z0-9_]*)+"))
            throw new IllegalArgumentException("Invalid package");
        return value;
    }
    public static String key(String value) {
        if(value==null||!value.matches("[0-9a-f]{64}"))throw new IllegalArgumentException("Invalid observed key");
        return value;
    }
}
