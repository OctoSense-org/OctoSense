package dev.makepad.octosense.wifi;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.Locale;

/** No passwords, arbitrary intents, settings keys or user IDs on this surface. */
public final class WifiSettingsContract {
    private WifiSettingsContract() {}
    public static final int MAX_NETWORKS=256;
    public enum Action {
        CONNECT("connect"), CONFIGURE("configure"), FORGET("forget");
        public final String wire;
        Action(String wire) {this.wire=wire;}
        public static Action parse(String value) {
            for(Action action:values()) if(action.wire.equals(value)) return action;
            throw new IllegalArgumentException("Unknown Wi-Fi action");
        }
    }
    public static String key(String value) {
        if(value==null||!value.matches("[0-9a-f]{64}")) throw new IllegalArgumentException("Invalid Wi-Fi target");
        return value;
    }
    public static boolean enabled(Object value) {
        if(!(value instanceof Boolean)) throw new IllegalArgumentException("Boolean Wi-Fi state required");
        return (Boolean)value;
    }
    public static String fingerprint(int networkId,String rawSsid,String security) {
        if(rawSsid==null||security==null) throw new IllegalArgumentException("Missing Wi-Fi identity");
        try {
            MessageDigest digest=MessageDigest.getInstance("SHA-256");
            // Length-prefixed fields preserve arbitrary SSID bytes and separators.
            for(String field:new String[]{Integer.toString(networkId),rawSsid,security}) {
                byte[] bytes=field.getBytes(StandardCharsets.UTF_8);
                digest.update(new byte[]{(byte)(bytes.length>>>24),(byte)(bytes.length>>>16),(byte)(bytes.length>>>8),(byte)bytes.length});
                digest.update(bytes);
            }
            StringBuilder out=new StringBuilder();
            for(byte value:digest.digest()) out.append(String.format(Locale.ROOT,"%02x",value&255));
            return out.toString();
        } catch(java.security.NoSuchAlgorithmException impossible) {throw new AssertionError(impossible);}
    }
    public static boolean compatible(String a,String b) {
        return a.equals(b)||(a.equals("wpa2_wpa3")&&(b.equals("wpa2")||b.equals("wpa3")))
                ||(b.equals("wpa2_wpa3")&&(a.equals("wpa2")||a.equals("wpa3")))
                ||(a.equals("open_owe")&&(b.equals("open")||b.equals("owe")))
                ||(b.equals("open_owe")&&(a.equals("open")||a.equals("owe")));
    }
    public static String mergedSecurity(String a,String b) {
        if(a.equals(b)) return a;
        if((a.equals("open")||a.equals("owe")||a.equals("open_owe"))
                &&(b.equals("open")||b.equals("owe")||b.equals("open_owe"))) return "open_owe";
        if((a.equals("wpa2")||a.equals("wpa3")||a.equals("wpa2_wpa3"))
                &&(b.equals("wpa2")||b.equals("wpa3")||b.equals("wpa2_wpa3"))) return "wpa2_wpa3";
        return "unknown";
    }
    public static String displaySsid(String raw) {
        if(raw==null||raw.isEmpty()||raw.equals("<unknown ssid>")) return null;
        StringBuilder out=new StringBuilder();
        raw.codePoints().filter(c -> !Character.isISOControl(c)&&!(c>=0x202a&&c<=0x202e)&&!(c>=0x2066&&c<=0x2069))
                .limit(128).forEach(out::appendCodePoint);
        return out.length()==0?null:out.toString();
    }
}
