import dev.makepad.octosense.notifications.AppNotificationsContract;
import dev.makepad.octosense.notifications.AppNotificationsContract.*;
public final class AppNotificationsContractTest {
    static void check(boolean value){if(!value)throw new AssertionError();}
    static void reject(Runnable action){try{action.run();throw new AssertionError();}catch(IllegalArgumentException expected){}}
    static final class Write implements LinkedWrite {
        boolean permitted=true,confirm=true,retire,failChannel,failApp;int channelCalls,appCalls;
        public boolean allowed(){return permitted;}
        public void channel()throws Exception{channelCalls++;if(failChannel)throw new Exception();if(retire)permitted=false;}
        public void app()throws Exception{appCalls++;if(failApp)throw new Exception();}
        public boolean confirmed(){return confirm;}
    }
    public static void main(String[] args){
        String key="a".repeat(64);
        AppNotificationsContract.page(1,20,key);AppNotificationsContract.packageName("android");
        for(String pkg:new String[]{"", "com.bad/intent", "com.$hidden", "com..bad", "com.1bad", "no_dot"})reject(()->AppNotificationsContract.packageName(pkg));
        reject(()->AppNotificationsContract.page(0,0,null));reject(()->AppNotificationsContract.page(1,20,null));reject(()->AppNotificationsContract.page(1,1,key));reject(()->AppNotificationsContract.page(1,6000,key));
        reject(()->AppNotificationsContract.key("A".repeat(64)));reject(()->Action.parse("permission_grant"));
        reject(()->AppNotificationsContract.value(Action.CHANNEL_IMPORTANCE,"none"));reject(()->AppNotificationsContract.value(Action.APP_ENABLED,"high"));
        check(AppNotificationsContract.appWritable(true,true,false,false,false));
        check(!AppNotificationsContract.appWritable(true,true,true,false,false));check(!AppNotificationsContract.appWritable(true,true,false,true,false));check(!AppNotificationsContract.appWritable(true,false,false,false,false));
        check(AppNotificationsContract.channelToggleWritable(true,false,false,false));check(!AppNotificationsContract.channelWritable(true,false,false));
        check(!AppNotificationsContract.channelToggleWritable(true,false,true,false));check(!AppNotificationsContract.channelWritable(false,true,true));
        check(AppNotificationsContract.text("Mail\u202e\n中文😀",20).equals("Mail中文😀"));
        Lease lease=new Lease();String first=lease.observe("app A + state 1",100);
        check(first.equals(lease.observe("app A + state 1",500)));check(!lease.claim(first,"app A + policy changed",600));
        check(lease.claim(first,"app A + state 1",600));check(!lease.claim(first,"app A + state 1",601));
        String second=lease.observe("app A + state 1",700);check(!first.equals(second));check(!lease.claim(first,"app A + state 1",701));
        check(!lease.claim(second,"app A + state 1",20701));check(!lease.claim(second,"app A + state 1",699));
        String third=lease.observe("reinstalled A",21000);check(!second.equals(third));lease.retire();check(!lease.claim(third,"reinstalled A",21001));
        Write write=new Write();check(AppNotificationsContract.linkedWrite(write).equals("notifications_applied"));check(write.channelCalls==1&&write.appCalls==1);
        write=new Write();write.retire=true;check(AppNotificationsContract.linkedWrite(write).equals("notifications_partial"));check(write.appCalls==0);
        write=new Write();write.failChannel=true;check(AppNotificationsContract.linkedWrite(write).equals("notifications_unconfirmed"));check(write.appCalls==0);
        write=new Write();write.failApp=true;check(AppNotificationsContract.linkedWrite(write).equals("notifications_partial"));
        write=new Write();write.confirm=false;check(AppNotificationsContract.linkedWrite(write).equals("notifications_unconfirmed"));
        write=new Write();write.permitted=false;check(AppNotificationsContract.linkedWrite(write).equals("notifications_restricted"));check(write.channelCalls==0&&write.appCalls==0);
    }
}
