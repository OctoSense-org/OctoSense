import dev.makepad.octosense.display.DisplaySettingsContract;
import dev.makepad.octosense.display.DisplaySettingsContract.Setting;
import java.util.List;

public final class DisplaySettingsContractTest {
    static void check(boolean value) { if (!value) throw new AssertionError(); }
    static void reject(Runnable work) {
        try { work.run(); } catch (IllegalArgumentException expected) { return; }
        throw new AssertionError("untrusted display value accepted");
    }
    public static void main(String[] args) {
        check(DisplaySettingsContract.densities(0,1080).isEmpty());
        check(DisplaySettingsContract.densities(420,0).isEmpty());
        for (int initial : new int[]{160,240,320,420,560,640}) {
            for (int width : new int[]{1080,1440,2160,2560}) {
                List<Integer> choices=DisplaySettingsContract.densities(initial,width);
                check(choices.size()<=7 && choices.contains(initial));
                int previous=0;
                for (int dpi:choices) {
                    check(dpi>previous);previous=dpi;
                    if(dpi>initial) check(width*160/dpi>=320 && dpi<=initial*1.5f);
                }
                // An externally forced custom value is not accidentally offered.
                check(!choices.contains(9999));
            }
        }
        String key=DisplaySettingsContract.fingerprint("display","0","420");
        check(DisplaySettingsContract.key(key).equals(key));
        check(!DisplaySettingsContract.fingerprint("ab","c").equals(DisplaySettingsContract.fingerprint("a","bc")));
        reject(()->DisplaySettingsContract.key(key.toUpperCase()));
        reject(()->DisplaySettingsContract.value(Setting.DENSITY,"420"));
        check(DisplaySettingsContract.encode(Setting.ACTIVATED,true).equals("on"));
        reject(()->DisplaySettingsContract.encode(Setting.ACTIVATED,"true"));
        reject(()->DisplaySettingsContract.encode(Setting.TEMPERATURE,2850.0));
        reject(()->DisplaySettingsContract.encode(Setting.MODE,true));
        reject(()->DisplaySettingsContract.value(Setting.MODE,"arbitrary.settings.key"));
        reject(()->Setting.parse("screen_brightness"));
        for(String bad:new String[]{"-1","+1","01","1.0","86400"," 1","１"})
            reject(()->DisplaySettingsContract.value(Setting.START,bad));
        check(DisplaySettingsContract.value(Setting.START,"0").equals("0"));
        check(DisplaySettingsContract.value(Setting.END,"86399").equals("86399"));
        check(DisplaySettingsContract.recent(10,20010));
        check(!DisplaySettingsContract.recent(10,20011));
        check(!DisplaySettingsContract.recent(-1,0));
        check(!DisplaySettingsContract.recent(10,9));
    }
}
