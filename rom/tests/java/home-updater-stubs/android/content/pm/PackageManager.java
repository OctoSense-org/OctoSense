package android.content.pm;
public class PackageManager {public final PackageInstaller installer=new PackageInstaller();public final PackageInfo info=new PackageInfo();
 public PackageInstaller getPackageInstaller(){return installer;}public PackageInfo getPackageInfo(String p,int f){return info;}}
