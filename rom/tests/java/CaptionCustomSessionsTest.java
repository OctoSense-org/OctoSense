package dev.makepad.octosense.agent;
import android.content.Context;import android.os.IBinder;import android.os.RemoteException;import org.json.JSONObject;
public class CaptionCustomSessionsTest{
    static class Token implements IBinder{boolean live=true;DeathRecipient callback;public boolean isBinderAlive(){return live;}public void linkToDeath(DeathRecipient d,int f)throws RemoteException{if(!live)throw new RemoteException();callback=d;}public boolean unlinkToDeath(DeathRecipient d,int f){return callback==d;}void die(){live=false;if(callback!=null)callback.binderDied();}}
    static void check(boolean b){if(!b)throw new AssertionError();}static String a="a".repeat(64),b="b".repeat(64);
    public static void main(String[] args)throws Exception{
        CaptionCustomSessions sessions=new CaptionCustomSessions(new Context());Token first=new Token(),replacement=new Token();
        check(sessions.snapshot(1,first,a,1,10).getBoolean("scope_active"));check(sessions.snapshot(2,first,a,1,10).getBoolean("scope_active"));
        check(!sessions.snapshot(3,replacement,b,1,11).getBoolean("scope_active"));check(sessions.apply(first,a,1,"caption_typeface","caption_typeface:serif",11).equals("control_unavailable"));check(sessions.apply(first,a,1,"caption_typeface","caption_typeface:serif",10).equals("control_applied"));
        IBinder.DeathRecipient late=first.callback;first.die();check(sessions.apply(first,a,1,"caption_typeface","caption_typeface:serif",10).equals("control_unavailable"));
        check(sessions.snapshot(4,replacement,b,1,11).getBoolean("scope_active"));late.binderDied();sessions.close(first,a,1,10);check(sessions.apply(replacement,b,1,"caption_typeface","caption_typeface:serif",11).equals("control_applied"));
        check(!sessions.snapshot(5,first,a,2,10).getBoolean("scope_active"));sessions.close(replacement,b,1,10);check(sessions.apply(replacement,b,1,"caption_typeface","caption_typeface:serif",11).equals("control_applied"));sessions.close(replacement,b,1,11);check(sessions.apply(replacement,b,1,"caption_typeface","caption_typeface:serif",11).equals("control_unavailable"));sessions.destroy();check(CaptionCustomPlatformSettings.writes==3&&CaptionCustomPlatformSettings.closes==2);
    }
}
