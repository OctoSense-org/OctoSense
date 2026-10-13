package android.os;
public class Bundle extends java.util.HashMap<String,Object> {
 public boolean getBoolean(String k,boolean d){Object v=get(k);return v instanceof Boolean?(Boolean)v:d;}
 public void putBoolean(String k,boolean v){put(k,v);}
}
