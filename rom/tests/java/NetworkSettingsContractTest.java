import dev.makepad.octosense.network.NetworkSettingsContract;
public final class NetworkSettingsContractTest {
    static void check(boolean value){if(!value)throw new AssertionError();}
    static void rejects(Runnable run){try {run.run();throw new AssertionError("Invalid hostname accepted");}catch(IllegalArgumentException expected){}}
    public static void main(String[] args){
        check(NetworkSettingsContract.hostname("DNS.Example.COM.").equals("dns.example.com"));
        check(NetworkSettingsContract.hostname("例子.中国").equals("xn--fsqu00a.xn--fiqs8s"));
        check(NetworkSettingsContract.hostname("dns").equals("dns"));
        for(String value:new String[]{null,"","1.1.1.1","::1","[::1]","https://dns.example","dns.example:853","a..example","-a.example","a-.example","dns.example/path"," dns.example","a\nb.example","a".repeat(64)+".example"})
            rejects(()->NetworkSettingsContract.hostname(value));
        rejects(()->NetworkSettingsContract.dnsHostname("automatic","dns.example"));
        rejects(()->NetworkSettingsContract.dnsHostname("hostname",null));
        rejects(()->NetworkSettingsContract.dnsHostname("invalid",null));
        check(NetworkSettingsContract.dnsHostname("off",null)==null);
        check(NetworkSettingsContract.observedMode(null,null).equals("automatic"));
        check(NetworkSettingsContract.observedMode("","off").equals("off"));
        check(NetworkSettingsContract.observedMode("typo","opportunistic")==null);
        check(NetworkSettingsContract.observedBoolean("2")==null);
        rejects(()->NetworkSettingsContract.enabled("true"));
    }
}
