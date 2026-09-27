package dev.makepad.octosense.battery;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import org.json.JSONArray;
import org.json.JSONObject;

/** The only battery optimization choices carried across the Settings boundary. */
public final class AppBatteryContract {
    private AppBatteryContract() {}
    public enum Mode {
        RESTRICTED("restricted"), OPTIMIZED("optimized"), UNRESTRICTED("unrestricted");
        public final String wire;
        Mode(String wire) { this.wire=wire; }
        public static Mode parse(String value) {
            for(Mode mode:values())if(mode.wire.equals(value))return mode;
            throw new IllegalArgumentException("Unknown app battery mode");
        }
    }
    public static String packageName(String value) {
        if(value==null||value.length()>255||!value.matches("[A-Za-z_][A-Za-z0-9_]*(\\.[A-Za-z_][A-Za-z0-9_]*)*")
                ||(!value.equals("android")&&!value.contains(".")))throw new IllegalArgumentException("Invalid package");
        return value;
    }
    public static String key(String value) {
        if(value==null||!value.matches("[0-9a-f]{64}"))throw new IllegalArgumentException("Invalid observed target");
        return value;
    }
    public static String hash(String... parts) {
        try {
            MessageDigest digest=MessageDigest.getInstance("SHA-256");
            for(String part:parts) {byte[] bytes=String.valueOf(part).getBytes(StandardCharsets.UTF_8);
                digest.update(Integer.toString(bytes.length).getBytes(StandardCharsets.US_ASCII));digest.update((byte)':');digest.update(bytes);}
            StringBuilder out=new StringBuilder();for(byte value:digest.digest())out.append(String.format(java.util.Locale.ROOT,"%02x",value&255));return out.toString();
        }catch(java.security.NoSuchAlgorithmException impossible){throw new AssertionError(impossible);}
    }
    public static JSONObject unavailable(long requestId,String pkg)throws Exception {
        if(requestId<=0)throw new IllegalArgumentException("Positive request ID required");packageName(pkg);
        return new JSONObject().put("schema",1).put("request_id",requestId).put("package",pkg)
            .put("availability","unavailable").put("reason","service_unavailable").put("shared_uid",false)
            .put("can_set",false).put("choices",new JSONArray());
    }
}
