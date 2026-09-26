package dev.makepad.octosense.settingsbroker;

import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Binder;
import android.os.Bundle;
import android.os.IBinder;
import android.os.Process;
import android.os.UserHandle;
import dev.makepad.octosense.keyboards.KeyboardContract;
import dev.makepad.octosense.keyboards.KeyboardJson;
import dev.makepad.octosense.keyboards.KeyboardPolicy;
import java.util.Arrays;

/** Exact Agent caller gate; no arbitrary keyboard/package/component can be selected by Binder. */
public final class OctoSenseKeyboardService extends Service {
    private void caller(){int uid=Binder.getCallingUid();String[] packages=getPackageManager().getPackagesForUid(uid);if(UserHandle.getUserId(uid)!=0||UserHandle.myUserId()!=0||packages==null||!Arrays.asList(packages).contains("dev.makepad.octosense.agent")||getPackageManager().checkSignatures(uid,Process.myUid())!=PackageManager.SIGNATURE_MATCH)throw new SecurityException("Caller is not the trusted Settings helper");}
    private static Bundle unavailable(){Bundle result=new Bundle();result.putBoolean("ok",false);return result;}
    private final IKeyboardSettings.Stub binder=new IKeyboardSettings.Stub(){
        @Override public Bundle snapshot(long requestId,String query,int offset){caller();KeyboardContract.read(requestId,query,offset);long identity=Binder.clearCallingIdentity();try{
            return KeyboardSession.onMain(()->{KeyboardSession session=KeyboardSession.get(OctoSenseKeyboardService.this);Bundle result=new Bundle();result.putBoolean("ok",true);result.putString("json",KeyboardJson.page(requestId,session.policy.snapshot(query,offset)).toString());return result;});
        }catch(Exception|LinkageError unknown){return unavailable();}finally{Binder.restoreCallingIdentity(identity);}}
        @Override public Bundle prepare(long requestId,String key,String target,String operation){caller();KeyboardPolicy.Action action=KeyboardContract.flow(requestId,key,target,operation);long identity=Binder.clearCallingIdentity();try{
            return KeyboardSession.onMain(()->{KeyboardSession session=KeyboardSession.get(OctoSenseKeyboardService.this);KeyboardPolicy.Review review=session.policy.prepare(key,target,action);if(review==null)return unavailable();
                Intent intent=new Intent(OctoSenseKeyboardService.this,OctoSenseKeyboardActivity.class).setData(Uri.parse("octosense-keyboard:"+review.ticket)).putExtra("ticket",review.ticket)
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_MULTIPLE_TASK|Intent.FLAG_ACTIVITY_EXCLUDE_FROM_RECENTS);
                PendingIntent flow=PendingIntent.getActivity(OctoSenseKeyboardService.this,0,intent,PendingIntent.FLAG_ONE_SHOT|PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_CANCEL_CURRENT);
                Bundle result=new Bundle();result.putBoolean("ok",true);result.putParcelable("flow",flow);return result;
            });
        }catch(Exception|LinkageError unknown){return unavailable();}finally{Binder.restoreCallingIdentity(identity);}}
    };
    @Override public IBinder onBind(Intent intent){return binder;}
}
