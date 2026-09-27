package dev.makepad.octosense.systemlanguage;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.HashSet;
import org.json.JSONArray;
import org.json.JSONObject;

/** Finite native languages and a reviewed order; never accepts caller-supplied locale tags. */
public final class SystemLanguageContract {
    private SystemLanguageContract() {}
    public static final int PAGE_SIZE=20,MAX_NODES=4096,MAX_DEPTH=3,MAX_CURRENT=128;
    public static String key(String value){if(value==null||!value.matches("[0-9a-f]{64}"))throw new IllegalArgumentException("Invalid observed language key");return value;}
    public static String optionalKey(String value){if(value==null)throw new IllegalArgumentException("Missing language key");return value.isEmpty()?value:key(value);}
    public static String query(String value){if(value==null||value.codePointCount(0,value.length())>80||value.codePoints().anyMatch(Character::isISOControl))throw new IllegalArgumentException("Invalid language filter");return value;}
    public static void read(long id,String key,String parent,String query,int offset){
        if(id<=0)throw new IllegalArgumentException("Positive request ID required");optionalKey(key);optionalKey(parent);query(query);
        if(offset<0||offset>=MAX_NODES||offset%PAGE_SIZE!=0||key.isEmpty()&&(!parent.isEmpty()||offset!=0))throw new IllegalArgumentException("Invalid language page");
    }
    public static void order(String key,String[] targets){
        key(key);if(targets==null||targets.length==0||targets.length>MAX_CURRENT)throw new IllegalArgumentException("At least one language is required");
        HashSet<String> seen=new HashSet<>();for(String target:targets)if(!seen.add(key(target)))throw new IllegalArgumentException("Duplicate language target");
    }
    public static String hash(String...parts){try{MessageDigest digest=MessageDigest.getInstance("SHA-256");for(String part:parts){byte[] bytes=part.getBytes(StandardCharsets.UTF_8);digest.update(java.nio.ByteBuffer.allocate(4).putInt(bytes.length).array());digest.update(bytes);}StringBuilder result=new StringBuilder();for(byte b:digest.digest())result.append(String.format(java.util.Locale.ROOT,"%02x",b&255));return result.toString();}catch(java.security.NoSuchAlgorithmException impossible){throw new AssertionError(impossible);}}
    public static JSONObject unavailable(long id,String query,String availability,String reason)throws Exception{
        return new JSONObject().put("schema",1).put("request_id",id).put("availability",availability).put("reason",reason)
            .put("can_apply",false).put("level","language").put("query",query(query)).put("offset",0).put("total",0).put("page_size",PAGE_SIZE).put("rows",new JSONArray());
    }
}
