import dev.makepad.octosense.controls.ColorAccessibility;
import dev.makepad.octosense.controls.ColorAccessibility.Setting;
import dev.makepad.octosense.controls.ColorAccessibility.Access;
import dev.makepad.octosense.controls.SettingsControlsContract.Control;
import dev.makepad.octosense.controls.SettingsControlsContract.Page;
import java.util.EnumMap;

public final class ColorAccessibilityTest {
    static void check(boolean value,String message) {if(!value)throw new AssertionError(message);}
    static void rejected(Runnable action) {try {action.run();throw new AssertionError("Expected rejection");}catch(IllegalArgumentException expected){}}
    static final class Provider implements ColorAccessibility.Store {
        final EnumMap<Setting,String> rows=new EnumMap<>(Setting.class);
        Access access=Access.WRITABLE;
        int writes,reads;
        boolean failRead,failWrite,delayWrite,failAfterWrite;
        Runnable onRead;
        public String read(Setting setting) {
            reads++;if(failRead)throw new IllegalStateException("Unavailable provider");
            if(onRead!=null){Runnable once=onRead;onRead=null;once.run();}
            return rows.get(setting);
        }
        public boolean write(Setting setting,int value) {
            writes++;if(failWrite)return false;
            if(!delayWrite)rows.put(setting,Integer.toString(value));
            if(failAfterWrite)failRead=true;
            return true;
        }
        ColorAccessibility.Backend backend(){return new ColorAccessibility.Backend(this,()->access);}
    }
    public static void main(String[] args) {
        Provider p=new Provider();ColorAccessibility.Backend backend=p.backend();
        check("off".equals(backend.read(Setting.INVERSION))&&"off".equals(backend.read(Setting.CORRECTION))&&"deuteranomaly".equals(backend.read(Setting.MODE)),"Verified missing defaults");
        check(p.rows.isEmpty()&&p.writes==0,"Read must not create provider rows");
        int[] nativeModes={12,11,13,0};String[] modes={"deuteranomaly","protanomaly","tritanomaly","grayscale"};
        for(int i=0;i<modes.length;i++) {
            check(Setting.MODE.stored(modes[i])==nativeModes[i],"Exact native mode mapping");
            check(modes[i].equals(Setting.MODE.observed(Integer.toString(nativeModes[i]))),"Exact native observation");
            check(Control.COLOR_CORRECTION_MODE.validate(modes[i]).equals(modes[i]),"Finite public choice");
        }
        for(Setting setting:Setting.values())for(String raw:new String[]{"","bogus","2147483648","-9","2"})check(setting.observed(raw)==null,"Unknown raw preference was normalized");
        rejected(()->Setting.MODE.stored("on"));rejected(()->Setting.INVERSION.stored("grayscale"));
        rejected(()->Control.parse(Page.PRIVACY,"color_inversion"));
        rejected(()->Control.COLOR_CORRECTION_MODE.validate("12"));
        check(Page.parse("accessibility_vision")==Page.ACCESSIBILITY_VISION,"Finite page");
        check("control_applied".equals(backend.apply(Setting.MODE,"grayscale")),"Save mode while correction is off");
        check(!p.rows.containsKey(Setting.CORRECTION)&&!p.rows.containsKey(Setting.INVERSION),"Mode selection changed unrelated flags");
        check("control_applied".equals(backend.apply(Setting.CORRECTION,"on")),"Correction enable readback");
        check("0".equals(p.rows.get(Setting.MODE)),"Enable must preserve selected mode");
        check("control_applied".equals(backend.apply(Setting.INVERSION,"on")),"Independent inversion");
        check("1".equals(p.rows.get(Setting.CORRECTION))&&"0".equals(p.rows.get(Setting.MODE)),"Inversion changed correction");
        p.rows.put(Setting.MODE,"99");int writes=p.writes;
        check(backend.read(Setting.MODE)==null&&"control_unavailable".equals(backend.apply(Setting.MODE,"deuteranomaly"))&&p.writes==writes,"Custom mode must not be coerced");
        p.rows.remove(Setting.MODE);p.access=Access.READ_ONLY;
        check("control_unavailable".equals(backend.apply(Setting.MODE,"deuteranomaly"))&&p.writes==writes,"Ordinary caller must not write");
        p.access=Access.RESTRICTED;
        check("policy_restricted".equals(backend.apply(Setting.MODE,"deuteranomaly"))&&p.writes==writes,"Locked state must not write");
        p.access=Access.WRITABLE;p.onRead=()->p.access=Access.READ_ONLY;
        check("control_unavailable".equals(backend.apply(Setting.MODE,"deuteranomaly"))&&p.writes==writes,"Recheck changed permission before write");
        p.access=Access.WRITABLE;p.onRead=()->p.access=Access.RESTRICTED;
        check("policy_restricted".equals(backend.apply(Setting.MODE,"deuteranomaly"))&&p.writes==writes,"Recheck user/lock before write");
        p.access=Access.WRITABLE;p.failRead=true;
        check("control_unavailable".equals(backend.apply(Setting.MODE,"deuteranomaly"))&&p.writes==writes,"Read error is not an absent row");
        p.failRead=false;p.failWrite=true;
        check("control_unavailable".equals(backend.apply(Setting.MODE,"grayscale")),"Failed write");
        p.failWrite=false;p.delayWrite=true;
        check("control_requested".equals(backend.apply(Setting.MODE,"grayscale"))&&!p.rows.containsKey(Setting.MODE),"Accepted write without readback must remain requested");
        p.delayWrite=false;p.failAfterWrite=true;
        check("control_requested".equals(backend.apply(Setting.MODE,"grayscale"))&&"0".equals(p.rows.get(Setting.MODE)),"Readback outage must not pretend rejected write");
        System.out.println("Color accessibility finite mappings, independent writes, raw-state, authority and readback tests passed");
    }
}
