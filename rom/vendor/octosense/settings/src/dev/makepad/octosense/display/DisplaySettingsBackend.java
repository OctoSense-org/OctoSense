package dev.makepad.octosense.display;

import android.app.KeyguardManager;
import android.content.Context;
import android.os.SystemClock;
import android.os.UserManager;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** Shared observed-target lifetime; platform-only APIs stay behind the helper implementation. */
public final class DisplaySettingsBackend {
    public interface Platform {
        JSONObject density(boolean writable) throws Exception;
        JSONObject night(boolean writable) throws Exception;
        boolean apply(DisplaySettingsContract.Setting setting,String value,JSONObject observed) throws Exception;
    }
    private final Context context;
    private final Platform platform;
    private final BooleanSupplier activeOwner;
    private String observed;
    private long observedAt=-1;
    public DisplaySettingsBackend(Context context,Platform platform,BooleanSupplier activeOwner) {
        this.context=context;this.platform=platform;this.activeOwner=activeOwner;
    }
    private boolean writable() {
        UserManager users=context.getSystemService(UserManager.class);
        KeyguardManager lock=context.getSystemService(KeyguardManager.class);
        return platform!=null&&activeOwner.getAsBoolean()&&users!=null&&users.isUserUnlocked()
            &&lock!=null&&!lock.isKeyguardLocked();
    }
    private JSONObject read() throws Exception {
        boolean writable=writable();
        JSONObject density=new JSONObject().put("availability","unavailable").put("can_set",false).put("options",new JSONArray());
        JSONObject night=new JSONObject().put("availability","unavailable").put("can_set",new JSONArray());
        if(platform==null) {
            int dpi=context.getResources().getDisplayMetrics().densityDpi;
            if(dpi>0)density.put("current_dpi",dpi);
        } else {
            try {density=platform.density(writable);}catch(SecurityException|IllegalStateException unavailable) {}
            try {night=platform.night(writable);}catch(SecurityException|IllegalStateException unavailable) {}
        }
        // Ordered fields include capabilities and actual values, so an external edit or revocation retires a draft.
        String key=DisplaySettingsContract.fingerprint("display-settings-v1",Integer.toString(android.os.Process.myUid()),density.toString(),night.toString());
        return new JSONObject().put("schema",1).put("key",key).put("density",density).put("night",night);
    }
    public synchronized JSONObject snapshot(long id) throws Exception {
        if(id<=0)throw new IllegalArgumentException("Positive request ID required");
        JSONObject state=read();observed=state.getString("key");observedAt=SystemClock.elapsedRealtime();
        return state.put("request_id",id);
    }
    public synchronized String set(String key,DisplaySettingsContract.Setting setting,String value) throws Exception {
        DisplaySettingsContract.key(key);DisplaySettingsContract.value(setting,value);
        if(!key.equals(observed)||!DisplaySettingsContract.recent(observedAt,SystemClock.elapsedRealtime()))return "display_target_changed";
        JSONObject before=read();
        if(!key.equals(before.getString("key")))return "display_target_changed";
        if(!writable())return "display_restricted";
        if(setting==DisplaySettingsContract.Setting.DENSITY) {
            JSONObject density=before.getJSONObject("density");boolean found=false;
            JSONArray options=density.getJSONArray("options");
            for(int i=0;i<options.length();i++)if(value.equals(options.getJSONObject(i).getString("key")))found=true;
            if(!density.getBoolean("can_set")||!found)return "display_unavailable";
        } else {
            JSONObject night=before.getJSONObject("night");JSONArray caps=night.getJSONArray("can_set");boolean found=false;
            for(int i=0;i<caps.length();i++)if(setting.wire.equals(caps.getString(i)))found=true;
            if(!found)return "display_unavailable";
            if(setting==DisplaySettingsContract.Setting.TEMPERATURE)DisplaySettingsContract.integer(value,night.getInt("min_kelvin"),night.getInt("max_kelvin"));
            if(setting==DisplaySettingsContract.Setting.MODE&&value.equals("sunset")&&!night.optBoolean("location_enabled",false))return "display_location_required";
        }
        observedAt=-1; // Claim once before changing the display, not after its configuration event.
        try {
            if(!platform.apply(setting,value,before))return "display_unavailable";
            JSONObject after=read();
            if(setting==DisplaySettingsContract.Setting.DENSITY) {
                JSONArray options=before.getJSONObject("density").getJSONArray("options");
                for(int i=0;i<options.length();i++) {JSONObject option=options.getJSONObject(i);
                    if(value.equals(option.getString("key")))return after.getJSONObject("density").optInt("current_dpi",-1)==option.getInt("dpi")?"display_applied":"display_requested";
                }
            }
            JSONObject night=after.getJSONObject("night");Object actual=night.opt(setting==DisplaySettingsContract.Setting.TEMPERATURE?"temperature_kelvin":setting==DisplaySettingsContract.Setting.START?"start_seconds":setting==DisplaySettingsContract.Setting.END?"end_seconds":setting.wire);
            String encoded=actual instanceof Boolean?((Boolean)actual?"on":"off"):String.valueOf(actual);
            return value.equals(encoded)?"display_applied":"display_requested";
        }catch(SecurityException restricted){return "display_restricted";}
    }
}
