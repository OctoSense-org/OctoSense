package dev.makepad.octosense.applanguage;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import org.json.JSONArray;
import org.json.JSONObject;

public final class AppLanguageContract {
    private AppLanguageContract() {}
    public static final int PAGE_SIZE=20,MAX_NODES=1024,MAX_DEPTH=3,MAX_CURRENT=128;
    public static String packageName(String value) {
        if(value==null||value.length()>255||!value.matches("[A-Za-z_][A-Za-z0-9_]*(\\.[A-Za-z_][A-Za-z0-9_]*)*")||(!value.equals("android")&&!value.contains(".")))throw new IllegalArgumentException("Invalid package");return value;
    }
    public static String key(String value) {if(value==null||!value.matches("[0-9a-f]{64}"))throw new IllegalArgumentException("Invalid observed key");return value;}
    public static String optionalKey(String value){if(value==null)throw new IllegalArgumentException("Missing key");return value.isEmpty()?value:key(value);}
    public static String query(String value){if(value==null||value.codePointCount(0,value.length())>128||value.codePoints().anyMatch(Character::isISOControl))throw new IllegalArgumentException("Invalid query");return value;}
    public static void read(long id,String pkg,String key,String parent,String query,int offset){if(id<=0)throw new IllegalArgumentException("Positive request ID required");packageName(pkg);optionalKey(key);optionalKey(parent);query(query);if(offset<0||offset>1020||offset%PAGE_SIZE!=0||key.isEmpty()&&(!parent.isEmpty()||offset!=0))throw new IllegalArgumentException("Invalid catalog page");}
    public static String hash(String...parts){try{MessageDigest digest=MessageDigest.getInstance("SHA-256");for(String part:parts){byte[] bytes=part.getBytes(StandardCharsets.UTF_8);digest.update(java.nio.ByteBuffer.allocate(4).putInt(bytes.length).array());digest.update(bytes);}StringBuilder text=new StringBuilder();for(byte b:digest.digest())text.append(String.format(java.util.Locale.ROOT,"%02x",b&255));return text.toString();}catch(java.security.NoSuchAlgorithmException impossible){throw new AssertionError(impossible);}}
    public static JSONObject unavailable(long id,String pkg,String query,String availability,String reason)throws Exception {
        return new JSONObject().put("schema",1).put("request_id",id).put("package",packageName(pkg)).put("availability",availability).put("reason",reason)
            .put("query",query(query)).put("offset",0).put("total",0).put("page_size",PAGE_SIZE).put("level","language").put("rows",new JSONArray()).put("can_set",false);
    }
}
