package android.app;
public class PendingIntent {
 public static final int FLAG_UPDATE_CURRENT=1,FLAG_MUTABLE=2;public static android.content.Intent lastIntent;public static int lastFlags;
 public static PendingIntent getActivity(android.content.Context c,int id,android.content.Intent intent,int flags,android.os.Bundle options){lastIntent=intent;lastFlags=flags;return new PendingIntent();}
 public android.content.IntentSender getIntentSender(){return new android.content.IntentSender();}
}
