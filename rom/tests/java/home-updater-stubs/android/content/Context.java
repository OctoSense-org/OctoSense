package android.content;
public class Context {
 public static final int MODE_PRIVATE=0;public static final SharedPreferences PREFS=new SharedPreferences();
 public static final android.content.pm.PackageManager PM=new android.content.pm.PackageManager();
 public static Intent started;
 public SharedPreferences getSharedPreferences(String name,int mode){return PREFS;}
 public android.content.pm.PackageManager getPackageManager(){return PM;}
 public String getPackageName(){return "dev.makepad.octosense";}
 public void startActivity(Intent intent){started=intent;}
}
