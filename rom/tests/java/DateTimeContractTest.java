import dev.makepad.octosense.datetime.DateTimeContract;
import java.time.Instant;
public final class DateTimeContractTest {
    interface Operation {void run();}
    private static void denied(Operation operation) {try {operation.run();throw new AssertionError("accepted invalid time");}catch(IllegalArgumentException expected){}}
    public static void main(String[] args) {
        long first=DateTimeContract.millis("2026-11-01 01:30","America/Los_Angeles","first",1970,2100);
        long second=DateTimeContract.millis("2026-11-01 01:30","America/Los_Angeles","second",1970,2100);
        if(first!=Instant.parse("2026-11-01T08:30:00Z").toEpochMilli()||second-first!=3600000) throw new AssertionError("repeated hour lost its explicit choice");
        denied(() -> DateTimeContract.millis("2026-03-08 02:30","America/Los_Angeles","first",1970,2100));
        denied(() -> DateTimeContract.millis("2026-04-31 12:00","UTC","first",1970,2100));
        denied(() -> DateTimeContract.millis("2100-02-29 12:00","UTC","first",1970,2100));
        denied(() -> DateTimeContract.millis("2026-01-01 24:00","UTC","first",1970,2100));
        denied(() -> DateTimeContract.millis("2026-01-01 12:60","UTC","first",1970,2100));
        denied(() -> DateTimeContract.millis("2026-01-01 12:00","GMT+08:00","first",1970,2100));
        denied(() -> DateTimeContract.millis("2026-01-01 12:00","UTC","random",1970,2100));
        denied(() -> DateTimeContract.civil("2026-01-01T12:00",1970,2100));
        denied(() -> DateTimeContract.civil("1969-12-31 23:59",1970,2100));
        denied(() -> DateTimeContract.Action.parse("set_global"));
        denied(() -> DateTimeContract.validate(DateTimeContract.Action.ZONE,"UTC","first"));
        denied(() -> DateTimeContract.validate(DateTimeContract.Action.AUTO_TIME,"true",null));
        long leap=DateTimeContract.millis("2000-02-29 12:00","UTC","first",1970,2100);
        if(leap!=Instant.parse("2000-02-29T12:00:00Z").toEpochMilli()) throw new AssertionError("leap day rejected");
        long shanghai=DateTimeContract.millis("2026-09-25 12:00","Asia/Shanghai","first",1970,2100);
        if(shanghai!=Instant.parse("2026-09-25T04:00:00Z").toEpochMilli()) throw new AssertionError("zone ignored");
    }
}
