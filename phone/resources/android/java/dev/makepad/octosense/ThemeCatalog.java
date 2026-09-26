package dev.makepad.octosense;

import android.content.Context;
import android.content.SharedPreferences;
import android.content.res.Configuration;
import android.graphics.Color;
import org.json.JSONArray;
import org.json.JSONObject;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/** One data catalog shared with mobile_theme.rs; no executable theme content. */
public final class ThemeCatalog {
    static final String PREFERENCES = "octosense-appearance";
    static final String CHOICE = "selection-v1";
    private static volatile ThemeCatalog instance;
    final List<Preset> presets;

    static final class Preset {
        final String id, name, description, descriptionZh, seed;
        final float radius;
        private final JSONObject light, dark;
        Preset(JSONObject json) throws Exception {
            id=json.getString("id"); name=json.getString("name");
            description=json.getString("description"); descriptionZh=json.getString("description_zh");
            seed=json.getString("seed"); radius=(float)json.getDouble("radius");
            light=json.getJSONObject("light"); dark=json.getJSONObject("dark");
            for(JSONObject mode:new JSONObject[]{light,dark}) for(String key:new String[]{"background","surface","surface_variant","text","muted","accent","on_accent","border","wallpaper_top","wallpaper_bottom"}) {
                String value=mode.getString(key);
                if(!value.matches("[0-9A-Fa-f]{6}")) throw new IllegalArgumentException("Invalid theme color");
            }
        }
        int color(String role, boolean night) {
            return Color.parseColor("#"+(night?dark:light).optString(role));
        }
    }
    static final class Choice {
        final String preset, appearance, wallpaper;
        Choice(String preset,String appearance,String wallpaper) {
            this.preset=preset; this.appearance=appearance; this.wallpaper=wallpaper;
        }
        static Choice defaults() { return new Choice("octosense","system","gradient"); }
        boolean dark(boolean systemDark) { return appearance.equals("dark") || appearance.equals("system") && systemDark; }
        Choice preset(String value) { return new Choice(value,appearance,wallpaper); }
        Choice appearance(String value) { return new Choice(preset,value,wallpaper); }
        Choice wallpaper(String value) { return new Choice(preset,appearance,value); }
        JSONObject json() {
            JSONObject json=new JSONObject();
            try {json.put("version",1).put("preset",preset).put("appearance",appearance).put("wallpaper",wallpaper);}
            catch(Exception impossible) {throw new IllegalStateException(impossible);}
            return json;
        }
        @Override public boolean equals(Object other) {
            if(!(other instanceof Choice)) return false;
            Choice c=(Choice)other;
            return preset.equals(c.preset)&&appearance.equals(c.appearance)&&wallpaper.equals(c.wallpaper);
        }
        @Override public int hashCode() { return java.util.Objects.hash(preset,appearance,wallpaper); }
    }
    private ThemeCatalog(Context context) throws Exception {
        byte[] bytes;
        try(InputStream stream=context.getAssets().open("makepad/octosense/resources/themes/mobile-presets.json");
            ByteArrayOutputStream buffer=new ByteArrayOutputStream()) {
            byte[] part=new byte[4096]; int n;
            while((n=stream.read(part))!=-1) buffer.write(part,0,n);
            bytes=buffer.toByteArray();
        }
        JSONObject catalog=new JSONObject(new String(bytes,StandardCharsets.UTF_8));
        if(catalog.getInt("version")!=1) throw new IllegalArgumentException("Unsupported theme catalog");
        JSONArray items=catalog.getJSONArray("presets");
        ArrayList<Preset> result=new ArrayList<>();
        for(int i=0;i<items.length();i++) result.add(new Preset(items.getJSONObject(i)));
        presets=Collections.unmodifiableList(result);
        preset(Choice.defaults().preset);
    }
    static ThemeCatalog get(Context context) {
        ThemeCatalog value=instance;
        if(value==null) synchronized(ThemeCatalog.class) {
            value=instance;
            if(value==null) try {instance=value=new ThemeCatalog(context.getApplicationContext());}
            catch(Exception e) {throw new IllegalStateException("Bundled themes could not be loaded",e);}
        }
        return value;
    }
    Preset preset(String id) {
        for(Preset preset:presets) if(preset.id.equals(id)) return preset;
        throw new IllegalArgumentException("Unknown preset");
    }
    Choice parse(String text) throws Exception {
        JSONObject json=new JSONObject(text);
        if(json.getInt("version")!=1) throw new IllegalArgumentException("Unsupported choice");
        String id=json.getString("preset"), appearance=json.getString("appearance"), wallpaper=json.getString("wallpaper");
        preset(id);
        if(!appearance.equals("system")&&!appearance.equals("light")&&!appearance.equals("dark")) throw new IllegalArgumentException("Unknown appearance");
        if(!wallpaper.equals("gradient")&&!wallpaper.equals("solid")) throw new IllegalArgumentException("Unknown background");
        return new Choice(id,appearance,wallpaper);
    }
    static SharedPreferences preferences(Context context) { return context.getSharedPreferences(PREFERENCES,Context.MODE_PRIVATE); }
    Choice read(Context context) {
        try {return parse(preferences(context).getString(CHOICE,""));}
        catch(Exception ignored) {return Choice.defaults();}
    }
    boolean save(Context context,Choice choice) {
        // Called on a worker: one atomic preference record for every field.
        try {parse(choice.json().toString());}
        catch(Exception e) {return false;}
        return preferences(context).edit().putString(CHOICE,choice.json().toString()).commit();
    }
    static boolean systemDark(Context context) {
        return (context.getResources().getConfiguration().uiMode&Configuration.UI_MODE_NIGHT_MASK)==Configuration.UI_MODE_NIGHT_YES;
    }
}
