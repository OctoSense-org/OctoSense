import dev.makepad.octosense.DeviceSetting;

/** Runs without Android: malformed script/native input must not become a provider write. */
public final class DeviceSettingContractTest {
    private static int checks;
    private static void rejects(Runnable action) {
        try { action.run(); } catch(IllegalArgumentException expected) {checks++;return;}
        throw new AssertionError("Invalid setting accepted");
    }
    public static void main(String[] args) {
        for(String arbitrary:new String[]{"adb_enabled","enabled_accessibility_services","screen_off_timeout","","system/font_scale"}) {
            rejects(() -> DeviceSetting.parse(arbitrary));
        }
        for(Object value:new Object[]{"30000",30_000.0,0,-1,Long.MAX_VALUE,30_001,true}) {
            rejects(() -> DeviceSetting.SCREEN_TIMEOUT.validate(value));
        }
        for(Object value:new Object[]{"1.15",Double.NaN,Double.POSITIVE_INFINITY,.2,4,1.1,false}) {
            rejects(() -> DeviceSetting.FONT_SCALE.validate(value));
        }
        for(DeviceSetting setting:DeviceSetting.values()) {
            if(setting.audio()) {
                for(Object value:new Object[]{"0.5",Double.NaN,Double.NEGATIVE_INFINITY,-.1,1.1,true}) rejects(() -> setting.validate(value));
                setting.validate(0);setting.validate(.5);setting.validate(1);
                rejects(() -> setting.storedValue(.5));
            } else if(setting!=DeviceSetting.FONT_SCALE&&setting!=DeviceSetting.SCREEN_TIMEOUT) {
                for(Object value:new Object[]{"true",1,0,null}) rejects(() -> setting.validate(value));
                setting.validate(true);setting.validate(false);
            }
            if(DeviceSetting.parse(setting.wire)!=setting) throw new AssertionError("Wire identity changed");
        }
        for(int timeout:new int[]{15000,30000,60000,120000,300000,600000}) DeviceSetting.SCREEN_TIMEOUT.validate(timeout);
        for(double scale:new double[]{.85,1,1.15,1.3,1.5}) DeviceSetting.FONT_SCALE.validate(scale);
        if(!"24".equals(DeviceSetting.HOUR_FORMAT.storedValue(true))||!"12".equals(DeviceSetting.HOUR_FORMAT.storedValue(false))) throw new AssertionError("Clock encoding");
        if(!"0".equals(DeviceSetting.AUTO_TIME.storedValue(false))) throw new AssertionError("Boolean encoding");
        System.out.println(checks+" invalid requests rejected; supported choices and encodings passed");
    }
}
