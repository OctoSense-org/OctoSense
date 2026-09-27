package dev.makepad.octosense.contracts;

import android.content.Context;
import android.net.ConnectivityManager;
import android.net.NetworkCapabilities;
import android.os.Bundle;

/** Live connection state without location, network names, identifiers, or credentials. */
public final class NetworkStatus {
    private NetworkStatus() {}
    public static Bundle read(Context context) {
        Bundle result = new Bundle();
        ConnectivityManager manager = context.getSystemService(ConnectivityManager.class);
        NetworkCapabilities network = manager == null ? null : manager.getNetworkCapabilities(manager.getActiveNetwork());
        boolean connected = network != null;
        boolean validated = connected && network.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED);
        boolean captive = connected && network.hasCapability(NetworkCapabilities.NET_CAPABILITY_CAPTIVE_PORTAL);
        boolean metered = connected && !network.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED);
        String transport = !connected ? "Offline" : network.hasTransport(NetworkCapabilities.TRANSPORT_VPN) ? "VPN"
                : network.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) ? "Wi-Fi"
                : network.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) ? "Mobile data"
                : network.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) ? "Ethernet" : "Network";
        String summary = !connected ? "Offline · Set up a connection"
                : transport + (captive ? " · Sign in required" : validated ? " · Connected" : " · No internet")
                    + (metered ? " · Metered" : "");
        result.putBoolean("network_connected", connected);
        result.putBoolean("network_validated", validated);
        result.putBoolean("network_captive", captive);
        result.putBoolean("network_metered", metered);
        result.putString("network_summary", summary);
        return result;
    }
}
