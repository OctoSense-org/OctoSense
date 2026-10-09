package android.content;
public class Intent {
 public static final String EXTRA_INTENT="intent";public static final int FLAG_ACTIVITY_NEW_TASK=1,FLAG_ACTIVITY_SINGLE_TOP=2;
 final java.util.Map<String,Object> extras=new java.util.HashMap<>();String action;public Class<?> target;public android.net.Uri data;
 public Intent(){}public Intent(Context c,Class<?> cls){target=cls;}
 public Intent setAction(String a){action=a;return this;}public String getAction(){return action;}
 public Intent setData(android.net.Uri d){data=d;return this;}public Intent putExtra(String k,Object v){extras.put(k,v);return this;}
 public String getStringExtra(String k){return (String)extras.get(k);}public int getIntExtra(String k,int d){Object v=extras.get(k);return v instanceof Integer?(Integer)v:d;}
 @SuppressWarnings("unchecked") public <T>T getParcelableExtra(String k){return (T)extras.get(k);}
 public Intent addFlags(int f){return this;}
}
