import dev.makepad.octosense.wifi.WifiSettingsContract;

public final class WifiSettingsContractTest {
    private static void require(boolean value) {if(!value) throw new AssertionError();}
    private static void rejects(Runnable fn) {
        try {fn.run();throw new AssertionError("accepted an invalid command");}
        catch(IllegalArgumentException expected) { }
    }
    public static void main(String[] args) {
        for(Object value:new Object[]{null,1,0,"true",1.0}) rejects(() -> WifiSettingsContract.enabled(value));
        require(WifiSettingsContract.enabled(true));require(!WifiSettingsContract.enabled(false));
        for(String action:new String[]{"connect","configure","forget"}) require(WifiSettingsContract.Action.parse(action).wire.equals(action));
        for(String action:new String[]{null,"shell","add_network","password","CONNECT"," connect"}) rejects(() -> WifiSettingsContract.Action.parse(action));
        String original=WifiSettingsContract.fingerprint(7,"办公 Wi-Fi","eap");
        require(WifiSettingsContract.key(original).equals(original));
        for(String key:new String[]{null,"",original.toUpperCase(),original+"0","7","\n"+original}) rejects(() -> WifiSettingsContract.key(key));
        // Android can reuse a numeric network ID after deletion. A new SSID or
        // security mode must never inherit an observed Forget target.
        require(!original.equals(WifiSettingsContract.fingerprint(7,"Guest","open")));
        require(!original.equals(WifiSettingsContract.fingerprint(7,"办公 Wi-Fi","open")));
        require(!original.equals(WifiSettingsContract.fingerprint(8,"办公 Wi-Fi","eap")));
        require(!WifiSettingsContract.fingerprint(1,"a\0b","c").equals(WifiSettingsContract.fingerprint(1,"a","b\0c")));
        require(WifiSettingsContract.compatible("wpa2_wpa3","wpa2"));
        require(WifiSettingsContract.compatible("wpa3","wpa2_wpa3"));
        require(!WifiSettingsContract.compatible("open","owe"));
        require(WifiSettingsContract.compatible("open_owe","owe"));
        require(WifiSettingsContract.mergedSecurity("open","owe").equals("open_owe"));
        require(WifiSettingsContract.mergedSecurity("wpa3","wpa2").equals("wpa2_wpa3"));
        require(WifiSettingsContract.mergedSecurity("wpa2_wpa3","wpa2").equals("wpa2_wpa3"));
        require(WifiSettingsContract.mergedSecurity("open","eap").equals("unknown"));
        require(!WifiSettingsContract.compatible("eap","wpa2"));
        require("办公 Wi-Fi".equals(WifiSettingsContract.displaySsid("办公 Wi-Fi")));
        require("abc".equals(WifiSettingsContract.displaySsid("a\nb\u202ec\u2066")));
        require(WifiSettingsContract.displaySsid("<unknown ssid>")==null);
        require(WifiSettingsContract.displaySsid("\n\u202e")==null);
        require(WifiSettingsContract.displaySsid("x".repeat(512)).length()==128);
        // Filtering a display name must not collapse distinct network targets.
        require(!WifiSettingsContract.fingerprint(1,"a\nb","open").equals(WifiSettingsContract.fingerprint(1,"ab","open")));
    }
}
