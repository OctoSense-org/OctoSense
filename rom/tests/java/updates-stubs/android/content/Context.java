package android.content;
public final class Context {
    public final android.app.KeyguardManager keyguard=new android.app.KeyguardManager();
    public final android.os.UserManager users=new android.os.UserManager();
    public final android.content.pm.PackageManager packages=new android.content.pm.PackageManager();
    public <T> T getSystemService(Class<T> type){return type.cast(type==android.app.KeyguardManager.class?keyguard:users);}
    public android.content.pm.PackageManager getPackageManager(){return packages;}
}
