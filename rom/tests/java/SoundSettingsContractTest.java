import dev.makepad.octosense.sounds.SoundSettingsContract;
import static dev.makepad.octosense.sounds.SoundSettingsContract.*;
public final class SoundSettingsContractTest {
    static void check(boolean test){if(!test)throw new AssertionError();}
    static void rejects(Runnable run){try{run.run();throw new AssertionError("Accepted invalid input");}catch(IllegalArgumentException expected){}}
    public static void main(String[] args){
        check(Type.parse("ringtone").androidType==1);check(Type.parse("notification").androidType==2);check(Type.parse("alarm").androidType==4);
        rejects(()->Type.parse("all"));rejects(()->Type.parse("../../ringtone"));rejects(()->key("content://media/internal/audio/media/1"));
        String catalog=fingerprint("catalog"),target=fingerprint("row"),other=fingerprint("unseen-row");
        check(offset(980,catalog)==980);rejects(()->offset(1000,catalog));rejects(()->offset(1,catalog));rejects(()->offset(20,null));
        Observed observed=new Observed(catalog,Type.RINGTONE,0,"old-default",1000);
        check(!observed.permits(catalog,Type.RINGTONE,0,"old-default",target,1100));observed.expose(target);
        check(observed.permits(catalog,Type.RINGTONE,0,"old-default",target,301000));
        check(!observed.permits(catalog,Type.RINGTONE,0,"old-default",target,301001));
        check(!observed.permits(catalog,Type.RINGTONE,0,"old-default",target,999));
        check(!observed.permits(catalog,Type.ALARM,0,"old-default",target,1100));
        check(!observed.permits(catalog,Type.RINGTONE,10,"old-default",target,1100));
        check(!observed.permits(catalog,Type.RINGTONE,0,"new-default",target,1100));
        check(!observed.permits(catalog,Type.RINGTONE,0,"old-default",other,1100));
        check(!observed.permits(other,Type.RINGTONE,0,"old-default",target,1100));
        check(!fingerprint("ab","c").equals(fingerprint("a","bc")));
    }
}
