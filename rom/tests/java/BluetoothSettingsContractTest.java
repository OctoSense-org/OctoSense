import dev.makepad.octosense.bluetooth.BluetoothSettingsContract;

public final class BluetoothSettingsContractTest {
    private static void rejected(Runnable action) {
        try {action.run();throw new AssertionError("Invalid value accepted");} catch(IllegalArgumentException expected) {}
    }
    public static void main(String[] args) {
        String key=BluetoothSettingsContract.fingerprint("AA:BB:CC:DD:EE:01");
        if(key.length()!=64||key.equals(BluetoothSettingsContract.fingerprint("AA:BB:CC:DD:EE:02"))) throw new AssertionError("Device identity collision");
        BluetoothSettingsContract.key(key);
        rejected(() -> BluetoothSettingsContract.key("AA:BB:CC:DD:EE:01"));
        rejected(() -> BluetoothSettingsContract.key("f".repeat(63)));
        rejected(() -> BluetoothSettingsContract.fingerprint("aa:bb:cc:dd:ee:01"));
        for(String value:new String[]{"", " ", "\u3000\u00a0", "a\n", "a\u0000b", "a".repeat(249),"中".repeat(83)})
            rejected(() -> BluetoothSettingsContract.name(value));
        BluetoothSettingsContract.name("中".repeat(82)+"ab");
        BluetoothSettingsContract.name("我的 OnePlus 6");
        for(String value:new String[]{"pair","cancel_pair","connect","disconnect","forget"}) BluetoothSettingsContract.Action.parse(value);
        rejected(() -> BluetoothSettingsContract.Action.parse("shell"));
        rejected(() -> BluetoothSettingsContract.sharingKind("location"));
        rejected(() -> BluetoothSettingsContract.sharingValue("true"));
        rejected(() -> BluetoothSettingsContract.enabled("true"));
        if(!BluetoothSettingsContract.enabled(Boolean.TRUE)) throw new AssertionError();
    }
}
