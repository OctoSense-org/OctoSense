import dev.makepad.octosense.notifications.NotificationHistoryContract;
public final class NotificationHistoryContractTest {
    interface Operation {void run();}
    static void denied(Operation op) {try {op.run();throw new AssertionError("accepted invalid page");}catch(IllegalArgumentException expected){}}
    public static void main(String[] args) {
        String key="a".repeat(64);
        NotificationHistoryContract.request(1,null,0);
        NotificationHistoryContract.request(2,key,1980);
        denied(()->NotificationHistoryContract.request(0,null,0));
        denied(()->NotificationHistoryContract.request(1,null,20));
        denied(()->NotificationHistoryContract.request(1,key,-20));
        denied(()->NotificationHistoryContract.request(1,key,1));
        denied(()->NotificationHistoryContract.request(1,key,2000));
        denied(()->NotificationHistoryContract.request(1,"A".repeat(64),0));
        denied(()->NotificationHistoryContract.request(1,key+"a",0));
        if(!NotificationHistoryContract.text(null,5).isEmpty())throw new AssertionError("null");
        if(!NotificationHistoryContract.text("中文📨📨tail",3).equals("中文📨"))throw new AssertionError("split surrogate");
        if(!NotificationHistoryContract.text("a\u0000\u0001\ud800b\udfff\n\tc",6).equals("ab\n\tc"))throw new AssertionError("unsafe control or surrogate");
    }
}
