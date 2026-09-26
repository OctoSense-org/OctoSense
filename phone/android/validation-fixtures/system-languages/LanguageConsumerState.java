package dev.makepad.octosense.systemlanguagesfixture;
import android.app.LocaleManager;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import org.json.JSONObject;

/** DUMP-gated observation only. Ordered broadcasts do not force-stop the consumer. */
public final class LanguageConsumerState extends BroadcastReceiver {
    public static JSONObject snapshot(Context context)throws Exception{
        android.content.SharedPreferences prefs=context.getSharedPreferences("consumer",Context.MODE_PRIVATE);
        return new JSONObject().put("system",context.getSystemService(LocaleManager.class).getSystemLocales().toLanguageTags())
            .put("created",prefs.getInt("created",0)).put("text",prefs.getString("text",""))
            .put("configuration",prefs.getString("configuration",""))
            .put("rendered_system",prefs.getString("system",""))
            .put("app",prefs.getString("app",""))
            .put("direction",prefs.getInt("direction",-1)).put("visible",prefs.getBoolean("visible",false))
            .put("focused",prefs.getBoolean("focused",false));
    }
    @Override public void onReceive(Context context,Intent intent){try{if(!"state".equals(intent.getStringExtra("operation")))throw new IllegalArgumentException("Finite observation required");setResultCode(0);setResultData(snapshot(context).toString());}catch(Exception failure){setResultCode(1);setResultData("unavailable");}}
}
