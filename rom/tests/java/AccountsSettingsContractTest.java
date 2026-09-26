import dev.makepad.octosense.accounts.AccountsSettingsContract;
import dev.makepad.octosense.accounts.AccountsSettingsContract.Observations;
import dev.makepad.octosense.accounts.AccountsSettingsContract.SyncAction;
public final class AccountsSettingsContractTest {
    private static void check(boolean value) {if(!value) throw new AssertionError();}
    private static void rejects(Runnable run) {try {run.run();throw new AssertionError("accepted invalid input");}catch(IllegalArgumentException expected) {}}
    public static void main(String[] args) {
        String a=AccountsSettingsContract.fingerprint("account",0,"mail","alice"),b=AccountsSettingsContract.fingerprint("account",0,"mail","bob"),c=AccountsSettingsContract.fingerprint("account",0,"mail","carol");
        check(AccountsSettingsContract.key(a).equals(a));
        check(!a.equals(AccountsSettingsContract.fingerprint("account",10,"mail","alice")));
        check(!AccountsSettingsContract.fingerprint("account",0,"ab","c").equals(AccountsSettingsContract.fingerprint("account",0,"a","bc")));
        check(!a.equals(AccountsSettingsContract.fingerprint("authority",0,"mail","alice")));
        for(String bad:new String[]{null,"alice@example.com",a.toUpperCase(),a.substring(1)}) rejects(()->AccountsSettingsContract.key(bad));
        rejects(()->SyncAction.parse("delete"));rejects(()->AccountsSettingsContract.enabled(1));
        rejects(()->AccountsSettingsContract.syncValue(SyncAction.AUTO,null));rejects(()->AccountsSettingsContract.syncValue(SyncAction.CANCEL,false));
        check(AccountsSettingsContract.syncValue(SyncAction.AUTO,true));check(AccountsSettingsContract.syncValue(SyncAction.SYNC_NOW,null)==null);
        long[] time={100};Observations seen=new Observations(2,30,()->time[0]);
        check(!seen.contains(a));seen.add(a);seen.add(b);check(seen.contains(a));seen.add(c);check(!seen.contains(a)&&seen.contains(b)&&seen.contains(c));
        time[0]=130;check(seen.contains(b));time[0]=131;check(!seen.contains(b));
        seen.add(a);time[0]=120;check(!seen.contains(a)); // monotonic-clock reset invalidates evidence
        time[0]=200;seen.add(a);seen.clear();check(!seen.contains(a));
        rejects(()->seen.add("forged"));
    }
}
