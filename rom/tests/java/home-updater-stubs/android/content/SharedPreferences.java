package android.content;
public class SharedPreferences {
 private final java.util.Map<String,Object> data=new java.util.HashMap<>();
 public String getString(String k,String d){Object v=data.get(k);return v instanceof String?(String)v:d;}
 public int getInt(String k,int d){Object v=data.get(k);return v instanceof Integer?(Integer)v:d;}
 public long getLong(String k,long d){Object v=data.get(k);return v instanceof Long?(Long)v:d;}
 public Editor edit(){return new Editor();}
 public class Editor {
  final java.util.Map<String,Object> changes=new java.util.HashMap<>();boolean clear;
  public Editor clear(){clear=true;return this;} public Editor putString(String k,String v){changes.put(k,v);return this;}
  public Editor putInt(String k,int v){changes.put(k,v);return this;}public Editor putLong(String k,long v){changes.put(k,v);return this;}
  public boolean commit(){if(clear)data.clear();data.putAll(changes);return true;}public void apply(){commit();}
 }
}
