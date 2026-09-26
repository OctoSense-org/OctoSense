package org.json;
/** In-memory JSON object for state-machine tests; deliberately has no parser. */
public final class JSONObject {
    public static final Object NULL=new Object();
    private final java.util.Map<String,Object> values=new java.util.LinkedHashMap<>();
    public JSONObject put(String key,Object value){values.put(key,value);return this;}
    public Object get(String key){if(!values.containsKey(key))throw new IllegalArgumentException(key);return values.get(key);}
    public JSONObject getJSONObject(String key){return (JSONObject)get(key);}
    public JSONArray getJSONArray(String key){return (JSONArray)get(key);}
    public String getString(String key){return (String)get(key);}
    public boolean getBoolean(String key){return (Boolean)get(key);}
    public boolean has(String key){return values.containsKey(key);}
    public String toString(){return values.toString();}
}
