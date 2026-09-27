package dev.makepad.octosense.updates;

import android.content.Context;
import android.os.Build;
import org.json.JSONObject;
import org.json.JSONArray;

/** Read-only installed versions also work outside the ROM. */
public final class UpdatesSettingsSnapshot {
    private UpdatesSettingsSnapshot() {}
    public static JSONObject current(Context context) throws Exception {
        JSONObject current=new JSONObject().put("rom_version",Build.VERSION.INCREMENTAL)
            .put("device",Build.DEVICE).put("model",Build.MODEL);
        try {current.put("home_version",context.getPackageManager().getPackageInfo("dev.makepad.octosense",0).getLongVersionCode());}
        catch(android.content.pm.PackageManager.NameNotFoundException ignored) {}
        return current;
    }
    public static JSONObject unavailable(Context context,long id,String availability) throws Exception {
        return new JSONObject().put("schema",1).put("request_id",id).put("availability",availability)
            .put("current",current(context)).put("check",new JSONObject().put("state","never"))
            .put("offer",JSONObject.NULL).put("rom",new JSONObject().put("phase","unavailable"))
            .put("home",new JSONObject().put("phase","unavailable")).put("capabilities",new JSONArray());
    }
}
