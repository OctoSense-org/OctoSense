package dev.makepad.octosense.appstorage;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import org.json.JSONArray;
import org.json.JSONObject;

/** Finite observed storage actions; no paths, raw keys or caller-selected intents. */
public final class AppStorageContract {
    private AppStorageContract() {}
    public enum Action {
        CLEAR_CACHE("clear_cache"), CLEAR_DATA("clear_data"), MANAGE_SPACE("manage_space");
        public final String wire;
        Action(String wire){this.wire=wire;}
        public static Action parse(String value){for(Action action:values())if(action.wire.equals(value))return action;throw new IllegalArgumentException("Unknown storage action");}
    }
    public static String packageName(String value){
        if(value==null||value.length()>255||!value.matches("[A-Za-z_][A-Za-z0-9_]*(\\.[A-Za-z_][A-Za-z0-9_]*)*")||(!value.equals("android")&&!value.contains(".")))throw new IllegalArgumentException("Invalid package");return value;
    }
    public static String key(String value){if(value==null||!value.matches("[0-9a-f]{64}"))throw new IllegalArgumentException("Invalid observed target");return value;}
    public static String hash(String... parts){try{MessageDigest digest=MessageDigest.getInstance("SHA-256");for(String part:parts){byte[] bytes=String.valueOf(part).getBytes(StandardCharsets.UTF_8);digest.update(Integer.toString(bytes.length).getBytes(StandardCharsets.US_ASCII));digest.update((byte)':');digest.update(bytes);}StringBuilder out=new StringBuilder();for(byte b:digest.digest())out.append(String.format(java.util.Locale.ROOT,"%02x",b&255));return out.toString();}catch(java.security.NoSuchAlgorithmException impossible){throw new AssertionError(impossible);}}
    public static JSONObject unavailable(long id,String pkg)throws Exception{if(id<=0)throw new IllegalArgumentException("Positive request ID required");packageName(pkg);return new JSONObject().put("schema",1).put("request_id",id).put("package",pkg).put("availability","unavailable").put("reason","service_unavailable").put("shared_uid",false).put("actions",new JSONArray());}
}
