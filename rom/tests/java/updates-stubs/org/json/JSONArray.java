package org.json;
public final class JSONArray {
    private final java.util.List<Object> values=new java.util.ArrayList<>();
    public JSONArray put(Object value){values.add(value);return this;}
    public boolean contains(Object value){return values.contains(value);}
}
