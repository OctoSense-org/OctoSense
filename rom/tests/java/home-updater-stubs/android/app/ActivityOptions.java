package android.app;
public class ActivityOptions {
 public static final int MODE_BACKGROUND_ACTIVITY_START_ALLOWED=1;public static int mode;
 public static ActivityOptions makeBasic(){return new ActivityOptions();}
 public ActivityOptions setPendingIntentCreatorBackgroundActivityStartMode(int m){mode=m;return this;}
 public android.os.Bundle toBundle(){return new android.os.Bundle();}
}
