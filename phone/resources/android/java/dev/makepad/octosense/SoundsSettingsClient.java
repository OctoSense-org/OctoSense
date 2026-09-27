package dev.makepad.octosense;

import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import android.provider.Settings;
import dev.makepad.octosense.agent.AgentPlatformClient;
import dev.makepad.octosense.sounds.SoundSettingsBackend;
import dev.makepad.octosense.sounds.SoundSettingsContract;
import java.util.HashSet;
import java.util.Set;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** One catalog chooses one backend. A failed mutation is never replayed elsewhere. */
final class SoundsSettingsClient implements AutoCloseable {
    private final Context context;
    private final AgentPlatformClient agent;
    private final SoundSettingsBackend local;
    private final BooleanSupplier foreground;
    private String observed,type;
    private boolean remote,canSave,canPreview;
    private final Set<String> targets=new HashSet<>();
    SoundsSettingsClient(Context context,AgentPlatformClient agent,BooleanSupplier foreground) {
        this.context=context;this.agent=agent;this.foreground=foreground;
        local=new SoundSettingsBackend(context,foreground);
    }
    private JSONObject denied(long id,String type,String status) throws Exception {
        invalidate();return new JSONObject().put("schema",1).put("request_id",id).put("type",type).put("status",status).put("key",JSONObject.NULL)
                .put("current",new JSONObject().put("state","unavailable").put("title",JSONObject.NULL))
                .put("offset",0).put("total",0).put("truncated",false).put("rows",new JSONArray())
                .put("can_save",false).put("can_preview",false).put("can_request_access",false);
    }
    synchronized JSONObject snapshot(long id,String type,String key,int offset) throws Exception {
        if(id<=0)throw new IllegalArgumentException("Positive request required");
        SoundSettingsContract.Type.parse(type);SoundSettingsContract.offset(offset,key);
        if(!foreground.getAsBoolean())return denied(id,type,"restricted");
        if(key==null) {invalidate();this.type=type;remote=agent.has("sounds_settings_v1");}
        else if(!key.equals(observed)||!type.equals(this.type))return denied(id,type,"expired");
        JSONObject state=remote?agent.soundsSnapshot(id,type,key,offset):local.snapshot(id,type,key,offset);
        if(state==null)return denied(id,type,"unavailable");
        if(!foreground.getAsBoolean())return denied(id,type,"restricted");
        if(!"ready".equals(state.getString("status"))) {observed=null;canSave=canPreview=false;targets.clear();return state;}
        observed=SoundSettingsContract.key(state.getString("key"));
        canSave=state.getBoolean("can_save");canPreview=state.getBoolean("can_preview");
        JSONArray rows=state.getJSONArray("rows");if(rows.length()>SoundSettingsContract.PAGE_SIZE)throw new IllegalStateException("Oversized sound page");
        for(int i=0;i<rows.length();i++)targets.add(SoundSettingsContract.key(rows.getJSONObject(i).getString("key")));
        // Home's special-access screen cannot grant privileges to the ROM service.
        if(remote)state.put("can_request_access",false);
        return state;
    }
    synchronized String action(String type,String key,String target,boolean save) throws Exception {
        SoundSettingsContract.Type.parse(type);SoundSettingsContract.key(key);SoundSettingsContract.key(target);
        if(!foreground.getAsBoolean()){invalidate();return "sound_restricted";}
        if(!key.equals(observed)||!type.equals(this.type)||!targets.contains(target))return "sound_target_changed";
        if(save?!canSave:!canPreview)return "sound_unavailable";
        String reason=remote?agent.soundAction(type,key,target,save):save?local.save(type,key,target):local.preview(type,key,target);
        if(save){observed=null;canSave=canPreview=false;targets.clear();}
        return reason;
    }
    synchronized String stop() {
        local.stop();try{return agent.soundStop();}catch(Exception unavailable){return "sound_unavailable";}
    }
    synchronized void invalidate() {stop();local.invalidate();observed=null;type=null;canSave=canPreview=false;targets.clear();}
    synchronized Intent access() {
        if(!foreground.getAsBoolean()||remote||Settings.System.canWrite(context))throw new SecurityException("Sound access is unavailable");
        return new Intent(Settings.ACTION_MANAGE_WRITE_SETTINGS,Uri.parse("package:"+context.getPackageName()));
    }
    @Override public synchronized void close(){invalidate();local.close();}
}
