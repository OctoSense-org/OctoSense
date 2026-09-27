import android.content.Context;
import android.os.SystemClock;
import dev.makepad.octosense.display.DisplaySettingsBackend;
import dev.makepad.octosense.display.DisplaySettingsContract;
import org.json.JSONArray;
import org.json.JSONObject;
public final class DisplaySettingsBackendTest {
 static void check(boolean ok,String message){if(!ok)throw new AssertionError(message);}
 static final String CHOICE=DisplaySettingsContract.fingerprint("choice");
 static final class Platform implements DisplaySettingsBackend.Platform {
  int current=420,calls,temperature=2850,start=79201;boolean authority=true,readback=true,nightEnabled,activated=true,location=false;String mode="custom";
  public JSONObject density(boolean writable){return new JSONObject().put("availability",writable?"available":"restricted").put("current_dpi",current).put("default_dpi",420).put("can_set",writable&&authority).put("options",new JSONArray().put(new JSONObject().put("key",CHOICE).put("label","Larger").put("dpi",504).put("is_default",false)));}
  public JSONObject night(boolean writable){if(!nightEnabled)return new JSONObject().put("availability","unsupported").put("can_set",new JSONArray());return new JSONObject().put("availability",writable?"available":"restricted").put("activated",activated).put("temperature_kelvin",temperature).put("min_kelvin",2000).put("max_kelvin",6500).put("mode",mode).put("start_seconds",start).put("end_seconds",21600).put("location_enabled",location).put("can_set",writable&&authority?new JSONArray().put("activated").put("temperature").put("mode").put("start").put("end"):new JSONArray());}
  public boolean apply(DisplaySettingsContract.Setting setting,String value,JSONObject observed){calls++;if(readback)switch(setting){case DENSITY:current=504;break;case TEMPERATURE:temperature=Integer.parseInt(value);break;case START:start=Integer.parseInt(value);break;case MODE:mode=value;break;case ACTIVATED:activated=value.equals("on");break;default:break;}return true;}
 }
 public static void main(String[] args)throws Exception{
  Context context=new Context();Platform platform=new Platform();boolean[] owner={true};
  DisplaySettingsBackend backend=new DisplaySettingsBackend(context,platform,()->owner[0]);
  String key=backend.snapshot(1).getString("key");platform.current=400;
  check(backend.set(key,DisplaySettingsContract.Setting.DENSITY,CHOICE).equals("display_target_changed"),"external change");check(platform.calls==0,"no stale mutation");
  key=backend.snapshot(2).getString("key");platform.authority=false;
  check(!backend.set(key,DisplaySettingsContract.Setting.DENSITY,CHOICE).equals("display_applied"),"revoked capability");check(platform.calls==0,"revocation no mutation");platform.authority=true;
  key=backend.snapshot(3).getString("key");owner[0]=false;
  check(!backend.set(key,DisplaySettingsContract.Setting.DENSITY,CHOICE).equals("display_applied"),"owner change");check(platform.calls==0,"wrong owner no mutation");owner[0]=true;
  key=backend.snapshot(4).getString("key");context.lock.locked=true;
  check(!backend.set(key,DisplaySettingsContract.Setting.DENSITY,CHOICE).equals("display_applied"),"lock change");check(platform.calls==0,"locked no mutation");context.lock.locked=false;
  key=backend.snapshot(5).getString("key");check(backend.set(key,DisplaySettingsContract.Setting.DENSITY,DisplaySettingsContract.fingerprint("forged")).equals("display_unavailable"),"unknown choice");check(platform.calls==0,"forged no mutation");
  key=backend.snapshot(6).getString("key");SystemClock.now+=20001;
  check(backend.set(key,DisplaySettingsContract.Setting.DENSITY,CHOICE).equals("display_target_changed"),"expired");check(platform.calls==0,"expired no mutation");
  key=backend.snapshot(7).getString("key");check(backend.set(key,DisplaySettingsContract.Setting.DENSITY,CHOICE).equals("display_applied"),"actual readback");
  check(backend.set(key,DisplaySettingsContract.Setting.DENSITY,CHOICE).equals("display_target_changed"),"double claim");check(platform.calls==1,"claimed once");
  platform.current=420;platform.readback=false;key=backend.snapshot(8).getString("key");check(backend.set(key,DisplaySettingsContract.Setting.DENSITY,CHOICE).equals("display_requested"),"accepted pending is not applied");check(platform.calls==2,"pending submitted once");
  platform.nightEnabled=true;platform.readback=true;key=backend.snapshot(10).getString("key");
  check(backend.set(key,DisplaySettingsContract.Setting.MODE,"sunset").equals("display_location_required"),"sunset cannot silently enable location");check(platform.calls==2,"no location side effect");
  boolean rangeRejected=false;try{backend.set(key,DisplaySettingsContract.Setting.TEMPERATURE,"1999");}catch(IllegalArgumentException expected){rangeRejected=true;}check(rangeRejected&&platform.calls==2,"actual reported temperature range");
  check(backend.set(key,DisplaySettingsContract.Setting.TEMPERATURE,"2750").equals("display_applied"),"temperature actual readback");
  key=backend.snapshot(11).getString("key");check(backend.set(key,DisplaySettingsContract.Setting.START,"76509").equals("display_applied"),"schedule seconds preserved");check(platform.start==76509,"no minute rounding");
  key=backend.snapshot(12).getString("key");check(backend.set(key,DisplaySettingsContract.Setting.ACTIVATED,"off").equals("display_applied"),"activation actual readback");
  DisplaySettingsBackend local=new DisplaySettingsBackend(context,null,()->true);JSONObject state=local.snapshot(9);check(!state.getJSONObject("density").getBoolean("can_set"),"ordinary local read only");check(state.getJSONObject("density").getInt("current_dpi")==420,"actual local dpi");
 }
}
