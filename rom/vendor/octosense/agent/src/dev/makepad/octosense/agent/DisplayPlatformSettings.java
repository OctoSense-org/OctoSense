package dev.makepad.octosense.agent;

import android.Manifest;
import android.content.Context;
import android.content.pm.PackageManager;
import android.hardware.display.ColorDisplayManager;
import android.hardware.display.DisplayManager;
import android.location.LocationManager;
import android.view.Display;
import android.view.DisplayInfo;
import android.view.IWindowManager;
import android.view.WindowManagerGlobal;
import dev.makepad.octosense.display.DisplaySettingsBackend;
import dev.makepad.octosense.display.DisplaySettingsContract;
import java.time.LocalTime;
import java.util.List;
import org.json.JSONArray;
import org.json.JSONObject;

/** Same default-display density and ColorDisplayManager APIs as the pinned Settings controllers. */
final class DisplayPlatformSettings implements DisplaySettingsBackend.Platform {
    private final Context context;
    DisplayPlatformSettings(Context context){this.context=context;}
    private boolean has(String permission){return context.checkSelfPermission(permission)==PackageManager.PERMISSION_GRANTED;}
    @Override public JSONObject density(boolean writable) throws Exception {
        JSONObject result=new JSONObject().put("availability","unavailable").put("can_set",false).put("options",new JSONArray());
        DisplayManager manager=context.getSystemService(DisplayManager.class);Display display=manager==null?null:manager.getDisplay(Display.DEFAULT_DISPLAY);
        DisplayInfo info=new DisplayInfo();
        if(display==null||!display.getDisplayInfo(info)||info.type!=Display.TYPE_INTERNAL)return result.put("availability","unsupported");
        IWindowManager window=WindowManagerGlobal.getWindowManagerService();int initial=window.getInitialDisplayDensity(Display.DEFAULT_DISPLAY);int current=window.getBaseDisplayDensity(Display.DEFAULT_DISPLAY);
        if(initial<=0||current<=0)return result;
        boolean allowed=writable&&has(Manifest.permission.WRITE_SECURE_SETTINGS);
        List<Integer> choices=DisplaySettingsContract.densities(initial,Math.min(info.logicalWidth,info.logicalHeight));
        JSONArray options=new JSONArray();
        for(int dpi:choices)options.put(new JSONObject().put("key",DisplaySettingsContract.fingerprint("default-display-density-v1",Integer.toString(initial),Integer.toString(info.logicalWidth),Integer.toString(info.logicalHeight),Integer.toString(dpi)))
            .put("label",dpi==initial?"Default":dpi<initial?"Smaller ("+Math.round(100f*dpi/initial)+"%)":"Larger ("+Math.round(100f*dpi/initial)+"%)")
            .put("dpi",dpi).put("is_default",dpi==initial));
        return result.put("availability",writable?"available":"restricted").put("current_dpi",current).put("default_dpi",initial)
            .put("can_set",allowed&&!choices.isEmpty()).put("options",options);
    }
    @Override public JSONObject night(boolean writable) throws Exception {
        JSONObject result=new JSONObject().put("availability","unavailable").put("can_set",new JSONArray());
        if(!ColorDisplayManager.isNightDisplayAvailable(context))return result.put("availability","unsupported");
        ColorDisplayManager color=context.getSystemService(ColorDisplayManager.class);if(color==null)return result;
        boolean activated=color.isNightDisplayActivated();int temperature=color.getNightDisplayColorTemperature();
        int min=ColorDisplayManager.getMinimumColorTemperature(context),max=ColorDisplayManager.getMaximumColorTemperature(context);int mode=color.getNightDisplayAutoMode();
        LocalTime start=color.getNightDisplayCustomStartTime(),end=color.getNightDisplayCustomEndTime();
        LocationManager location=context.getSystemService(LocationManager.class);
        JSONArray caps=new JSONArray();boolean allowed=writable&&has("android.permission.CONTROL_DISPLAY_COLOR_TRANSFORMS");
        if(allowed){caps.put("activated").put("mode");if(activated&&min>0&&max>=min&&temperature>=min&&temperature<=max)caps.put("temperature");if(mode==1&&start!=null&&end!=null)caps.put("start").put("end");}
        result.put("availability",writable?"available":"restricted").put("activated",activated).put("can_set",caps);
        if(temperature>0)result.put("temperature_kelvin",temperature);
        if(min>0&&max>=min)result.put("min_kelvin",min).put("max_kelvin",max);
        if(mode>=0&&mode<=2)result.put("mode",mode==0?"disabled":mode==1?"custom":"sunset");
        if(start!=null)result.put("start_seconds",start.toSecondOfDay());if(end!=null)result.put("end_seconds",end.toSecondOfDay());
        if(location!=null)result.put("location_enabled",location.isLocationEnabled());return result;
    }
    @Override public boolean apply(DisplaySettingsContract.Setting setting,String value,JSONObject observed) throws Exception {
        if(setting==DisplaySettingsContract.Setting.DENSITY) {
            JSONArray options=observed.getJSONObject("density").getJSONArray("options");
            for(int i=0;i<options.length();i++){JSONObject option=options.getJSONObject(i);if(!value.equals(option.getString("key")))continue;
                IWindowManager window=WindowManagerGlobal.getWindowManagerService();
                if(option.getBoolean("is_default"))window.clearForcedDisplayDensityForUser(Display.DEFAULT_DISPLAY,0);
                else window.setForcedDisplayDensityForUser(Display.DEFAULT_DISPLAY,option.getInt("dpi"),0);
                return true;
            }return false;
        }
        ColorDisplayManager color=context.getSystemService(ColorDisplayManager.class);if(color==null)return false;
        switch(setting){
            case ACTIVATED:return color.setNightDisplayActivated(value.equals("on"));
            case TEMPERATURE:return color.setNightDisplayColorTemperature(Integer.parseInt(value));
            case MODE:return color.setNightDisplayAutoMode(value.equals("disabled")?0:value.equals("custom")?1:2);
            case START:return color.setNightDisplayCustomStartTime(LocalTime.ofSecondOfDay(Integer.parseInt(value)));
            case END:return color.setNightDisplayCustomEndTime(LocalTime.ofSecondOfDay(Integer.parseInt(value)));
            default:return false;
        }
    }
}
