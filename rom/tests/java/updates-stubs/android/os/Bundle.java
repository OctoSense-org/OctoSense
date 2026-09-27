package android.os;
public final class Bundle extends java.util.HashMap<String,Object> {
    public void putString(String k,String v){put(k,v);}public void putLong(String k,long v){put(k,v);}
    public void putBoolean(String k,boolean v){put(k,v);}public void putFloat(String k,float v){put(k,v);}
    public String getString(String k){return getString(k,null);}public String getString(String k,String d){return containsKey(k)?(String)get(k):d;}
    public boolean getBoolean(String k){return Boolean.TRUE.equals(get(k));}
    public long getLong(String k){return containsKey(k)?((Number)get(k)).longValue():0;}
    public float getFloat(String k,float d){return containsKey(k)?((Number)get(k)).floatValue():d;}
}
