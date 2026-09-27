package dev.makepad.octosense.keyboardsfixture;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import org.json.JSONObject;

public final class EditorObservation extends BroadcastReceiver {
    @Override public void onReceive(Context context, Intent intent) {
        try {
            if (!"state".equals(intent.getStringExtra("operation"))) throw new IllegalArgumentException("Finite observation required");
            SharedPreferences prefs = context.getSharedPreferences("observation", Context.MODE_PRIVATE);
            setResultCode(0);
            String state = new JSONObject().put("text", prefs.getString("text", ""))
                    .put("editor_focused", prefs.getBoolean("editor_focused", false))
                    .put("window_focused", prefs.getBoolean("window_focused", false))
                    .put("visible", prefs.getBoolean("visible", false)).toString();
            setResultData(android.util.Base64.encodeToString(state.getBytes(java.nio.charset.StandardCharsets.UTF_8), android.util.Base64.NO_WRAP));
        } catch (Exception failure) {
            setResultCode(1);
            setResultData("unavailable");
        }
    }
}
