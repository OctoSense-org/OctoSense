package android.content.pm;
public final class PackageManager {
    public static final class NameNotFoundException extends Exception {}
    public PackageInfo getPackageInfo(String name,int flags) throws NameNotFoundException {return new PackageInfo();}
}
