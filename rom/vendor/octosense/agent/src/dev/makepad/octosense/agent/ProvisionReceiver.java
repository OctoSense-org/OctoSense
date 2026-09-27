package dev.makepad.octosense.agent;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.provider.Settings;
import android.util.Log;

/**
 * First-boot provisioning of the OctoSense look: the system palette is seeded
 * with the launcher's purple (0x6750A4) so SystemUI's shade, dialogs and the
 * Settings app share the launcher's language. Done once per user; the person
 * may change the colour afterwards and it is not re-applied.
 *
 * SettingsProvider has no default for this setting, so the ROM sets it here.
 */
public class ProvisionReceiver extends BroadcastReceiver {
    static final String PROVISIONED = "octosense_provisioned";
    static final String PALETTE = "{\"android.theme.customization.system_palette\":\"6750A4\","
            + "\"android.theme.customization.accent_color\":\"6750A4\","
            + "\"android.theme.customization.theme_style\":\"TONAL_SPOT\","
            + "\"android.theme.customization.color_source\":\"preset\","
            + "\"android.theme.customization.color_both\":\"1\"}";

    @Override public void onReceive(Context context, Intent intent) {
        if (!Intent.ACTION_BOOT_COMPLETED.equals(intent.getAction())) return;
        try {
            if (Settings.Secure.getInt(context.getContentResolver(), PROVISIONED, 0) >= 1) return;
            Settings.Secure.putString(context.getContentResolver(),
                    Settings.Secure.THEME_CUSTOMIZATION_OVERLAY_PACKAGES, PALETTE);
            Settings.Secure.putInt(context.getContentResolver(), PROVISIONED, 1);
            Log.i(AgentPlatformService.TAG, "provisioned the OctoSense palette");
        } catch (RuntimeException e) {
            Log.w(AgentPlatformService.TAG, "provisioning failed", e);
        }
    }
}
