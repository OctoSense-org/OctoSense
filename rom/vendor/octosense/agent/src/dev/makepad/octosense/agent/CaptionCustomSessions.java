package dev.makepad.octosense.agent;

import android.content.Context;
import android.os.IBinder;
import android.os.RemoteException;
import dev.makepad.octosense.controls.CaptionCustomContract;
import dev.makepad.octosense.controls.CaptionCustomSettings.Field;
import org.json.JSONArray;
import org.json.JSONObject;

/** One authenticated Home process owns an editor cohort. Binder death cannot retire its successor. */
final class CaptionCustomSessions {
    private final CaptionCustomPlatformSettings settings;
    private IBinder owner;private String session;private int pid;private IBinder.DeathRecipient death;
    CaptionCustomSessions(Context context){settings=new CaptionCustomPlatformSettings(context);}
    private void clear(){if(owner!=null&&death!=null)try{owner.unlinkToDeath(death,0);}catch(java.util.NoSuchElementException alreadyDead){}if(session!=null)settings.invalidateSession(session);owner=null;session=null;pid=0;death=null;}
    synchronized void destroy(){clear();}
    private boolean matches(IBinder token,String identity,int caller){return token!=null&&token.equals(owner)&&identity!=null&&identity.equals(session)&&caller==pid&&token.isBinderAlive();}
    private boolean register(IBinder token,String identity,int caller)throws RemoteException{
        if(token==null||!token.isBinderAlive()||identity==null||!identity.matches("[0-9a-f]{64}")||caller<=0)return false;
        if(owner!=null&&owner.isBinderAlive())return matches(token,identity,caller);
        clear();final IBinder captured=token;final String capturedSession=identity;
        IBinder.DeathRecipient recipient=()->{synchronized(CaptionCustomSessions.this){if(captured.equals(owner)&&capturedSession.equals(session))clear();}};
        token.linkToDeath(recipient,0);if(!token.isBinderAlive()){token.unlinkToDeath(recipient,0);return false;}
        owner=token;session=identity;pid=caller;death=recipient;settings.replaceSession(identity);return true;
    }
    synchronized JSONObject snapshot(long id,IBinder token,String identity,long visit,int caller)throws Exception{
        CaptionCustomContract.read(id,visit);boolean entered=register(token,identity,caller)&&settings.enterScope(identity,visit);
        JSONArray rows=new JSONArray();
        for(Field field:Field.values()){
            String value=settings.read(field);JSONArray options=new JSONArray();
            if(entered&&value!=null)for(String choice:settings.choices(identity,visit,field))options.put(choice);
            rows.put(new JSONObject().put("id",field.id).put("value",value==null?JSONObject.NULL:value).put("options",options).put("availability",value==null?"unavailable":"available"));
        }
        Boolean custom=settings.customSelected();boolean active=entered&&matches(token,identity,caller)&&settings.scopeActive(identity,visit);
        if(!active||!Boolean.TRUE.equals(custom))for(int i=0;i<rows.length();i++)rows.getJSONObject(i).put("options",new JSONArray());
        return CaptionCustomContract.envelope(id,visit,custom,active,rows);
    }
    synchronized String apply(IBinder token,String identity,long visit,String field,String value,int caller){
        Field parsed=Field.parse(field);parsed.validate(value);
        return matches(token,identity,caller)?settings.apply(identity,visit,parsed,value):"control_unavailable";
    }
    synchronized void close(IBinder token,String identity,long visit,int caller){if(matches(token,identity,caller))settings.leaveScope(identity,visit);}
}
