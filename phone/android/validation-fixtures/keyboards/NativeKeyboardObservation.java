package dev.makepad.octosense.keyboardsfixture;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ApplicationInfo;
import android.view.inputmethod.InputMethodInfo;
import android.view.inputmethod.InputMethodManager;
import android.view.inputmethod.InputMethodSubtype;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import org.json.JSONArray;
import org.json.JSONObject;

/** Independent public-SDK native observations. No product Binder or keyboard setters. */
public final class NativeKeyboardObservation extends BroadcastReceiver {
    static JSONArray subtypes(List<InputMethodSubtype> values) throws Exception {
        JSONArray result = new JSONArray();
        for (InputMethodSubtype value : values) {
            result.put(new JSONObject().put("hash", value.hashCode())
                    .put("language_tag", value.getLanguageTag())
                    .put("locale", value.getLocale()).put("mode", value.getMode())
                    .put("ascii", value.isAsciiCapable()).put("auxiliary", value.isAuxiliary()));
        }
        return result;
    }

    static JSONObject snapshot(Context context) throws Exception {
        InputMethodManager manager = context.getSystemService(InputMethodManager.class);
        List<InputMethodInfo> installed = new ArrayList<>(manager.getInputMethodList());
        installed.sort(Comparator.comparing(InputMethodInfo::getId));
        List<String> enabled = new ArrayList<>();
        for (InputMethodInfo info : manager.getEnabledInputMethodList()) enabled.add(info.getId());
        java.util.Collections.sort(enabled);
        JSONArray providers = new JSONArray();
        for (InputMethodInfo info : installed) {
            List<InputMethodSubtype> declared = new ArrayList<>();
            for (int index = 0; index < info.getSubtypeCount(); index++) declared.add(info.getSubtypeAt(index));
            providers.put(new JSONObject().put("id", info.getId()).put("package", info.getPackageName())
                    .put("label", info.loadLabel(context.getPackageManager()).toString())
                    .put("direct_boot", info.getServiceInfo().directBootAware)
                    .put("system", (info.getServiceInfo().applicationInfo.flags & ApplicationInfo.FLAG_SYSTEM) != 0)
                    .put("settings_activity", info.getSettingsActivity() == null ? JSONObject.NULL : info.getSettingsActivity())
                    .put("subtypes", subtypes(declared))
                    .put("enabled_subtypes", subtypes(manager.getEnabledInputMethodSubtypeList(info, true))));
        }
        InputMethodInfo current = manager.getCurrentInputMethodInfo();
        InputMethodSubtype subtype = manager.getCurrentInputMethodSubtype();
        return new JSONObject().put("installed", providers).put("enabled", new JSONArray(enabled))
                .put("current_id", current == null ? JSONObject.NULL : current.getId())
                .put("current_subtype_hash", subtype == null ? JSONObject.NULL : subtype.hashCode());
    }

    @Override public void onReceive(Context context, Intent intent) {
        try {
            if (!"state".equals(intent.getStringExtra("operation"))) throw new IllegalArgumentException("Finite observation required");
            String state = snapshot(context).toString();
            setResultCode(0);
            setResultData(android.util.Base64.encodeToString(state.getBytes(java.nio.charset.StandardCharsets.UTF_8), android.util.Base64.NO_WRAP));
        } catch (Exception failure) {
            setResultCode(1);
            setResultData(failure.getClass().getSimpleName());
        }
    }
}
