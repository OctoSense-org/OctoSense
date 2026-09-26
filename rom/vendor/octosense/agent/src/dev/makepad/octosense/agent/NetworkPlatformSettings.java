package dev.makepad.octosense.agent;

import android.content.Context;
import android.net.ConnectivityManager;
import android.net.ConnectivitySettingsManager;
import android.net.LinkProperties;
import android.net.NetworkPolicyManager;
import dev.makepad.octosense.network.NetworkSettingsBackend;

/** Same APIs used by the pinned Android Settings network controllers. */
final class NetworkPlatformSettings implements NetworkSettingsBackend.Platform {
    private final Context context;
    NetworkPlatformSettings(Context context) {this.context=context;}
    @Override public Boolean dataSaver() {
        NetworkPolicyManager policy=context.getSystemService(NetworkPolicyManager.class);
        return policy==null?null:policy.getRestrictBackground();
    }
    @Override public Boolean dnsValidated(LinkProperties properties) {return properties==null?null:!properties.getValidatedPrivateDnsServers().isEmpty();}
    @Override public boolean airplane(boolean value) {
        ConnectivityManager manager=context.getSystemService(ConnectivityManager.class);
        if(manager==null) return false;manager.setAirplaneMode(value);return true;
    }
    @Override public boolean dataSaver(boolean value) {
        NetworkPolicyManager policy=context.getSystemService(NetworkPolicyManager.class);
        if(policy==null) return false;policy.setRestrictBackground(value);return true;
    }
    @Override public boolean privateDns(String mode,String hostname) {
        // Android saves the hostname first and preserves it when changing mode.
        if("hostname".equals(mode)) ConnectivitySettingsManager.setPrivateDnsHostname(context,hostname);
        ConnectivitySettingsManager.setPrivateDnsMode(context,"off".equals(mode)?ConnectivitySettingsManager.PRIVATE_DNS_MODE_OFF
            :"automatic".equals(mode)?ConnectivitySettingsManager.PRIVATE_DNS_MODE_OPPORTUNISTIC:ConnectivitySettingsManager.PRIVATE_DNS_MODE_PROVIDER_HOSTNAME);
        return true;
    }
}
