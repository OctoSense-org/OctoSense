import dev.makepad.octosense.battery.AppBatteryBackend;
import dev.makepad.octosense.battery.AppBatteryContract;
import dev.makepad.octosense.battery.AppBatteryContract.Mode;
import org.json.JSONObject;

public final class AppBatteryBackendTest {
    static int checks;
    static void check(boolean value){checks++;if(!value)throw new AssertionError("Check "+checks);}
    static final class Platform implements AppBatteryBackend.Platform {
        int any=0,in=0,writes;boolean allow,preO,mutable=true,shared,pending,fail,throwWrite;
        String identity="install-A",availability="available",reason="none";
        public AppBatteryBackend.State read(String pkg){
            AppBatteryBackend.State s=new AppBatteryBackend.State();s.packageName=pkg;s.label="Fixture";
            s.availability=availability;s.reason=reason;s.canSet=mutable;s.sharedUid=shared;s.preO=preO;
            s.runAny=any;s.runIn=preO?in:null;s.allowlisted=allow;s.incarnation=identity;
            s.revision=AppBatteryContract.hash(identity,""+any,""+in,""+allow,""+mutable,""+shared,""+preO,""+availability);return s;
        }
        public AppBatteryBackend.Write apply(AppBatteryBackend.State state,Mode mode){
            writes++;if(throwWrite)throw new IllegalStateException("Native dependency failed");
            if(pending)return AppBatteryBackend.Write.ACCEPTED;
            any=mode==Mode.RESTRICTED?1:0;
            if(fail)return AppBatteryBackend.Write.PARTIAL;
            if(preO)in=any;allow=mode==Mode.UNRESTRICTED;return AppBatteryBackend.Write.ACCEPTED;
        }
    }
    static String key(AppBatteryBackend b,long id)throws Exception{return b.snapshot(id,"fixture.battery").getString("key");}
    public static void main(String[] args)throws Exception{
        String pkg="fixture.battery";long[] now={10};Platform p=new Platform();AppBatteryBackend b=new AppBatteryBackend(p,()->now[0]);
        String key=key(b,1);check(key.equals(key(b,2)));check(b.snapshot(3,pkg).getJSONArray("choices").length()==3);
        for(Mode m:Mode.values()){
            key=key(b,4);check(b.set(pkg,key,m.wire).equals("app_battery_applied"));check(p.read(pkg).matches(m));
            int count=p.writes;check(b.set(pkg,key,m.wire).equals("app_battery_target_changed"));check(p.writes==count);
        }
        p.preO=true;p.in=2;check(b.snapshot(5,pkg).getString("mode").equals("custom"));
        key=key(b,6);check(b.set(pkg,key,"optimized").equals("app_battery_applied"));check(p.in==0&&!p.allow&&p.any==0);
        p.any=1;p.allow=true;check(b.snapshot(7,pkg).getString("mode").equals("custom"));
        key=key(b,8);check(b.set(pkg,key,"restricted").equals("app_battery_applied"));check(p.in==1&&p.any==1&&!p.allow);
        p.preO=false;p.any=4;check(b.snapshot(9,pkg).getString("mode").equals("custom"));
        key=key(b,10);check(b.set(pkg,key,"unrestricted").equals("app_battery_applied"));
        key=key(b,11);p.identity="install-B";int writes=p.writes;check(b.set(pkg,key,"restricted").equals("app_battery_target_changed"));check(p.writes==writes);
        key=key(b,12);p.any=1;check(b.set(pkg,key,"optimized").equals("app_battery_target_changed"));check(p.writes==writes);
        key=key(b,13);now[0]+=AppBatteryBackend.TTL_MS+1;check(b.set(pkg,key,"optimized").equals("app_battery_target_changed"));
        key=key(b,14);now[0]--;check(b.set(pkg,key,"optimized").equals("app_battery_target_changed"));now[0]+=2;
        key=key(b,15);check(b.set(pkg,"0".repeat(64),"optimized").equals("app_battery_target_changed"));
        check(b.set("other.package",key,"optimized").equals("app_battery_target_changed"));check(p.writes==writes);
        for(String why:new String[]{"device_policy","protected_app","locked"}){
            p.mutable=true;p.reason="none";key=key(b,16);p.mutable=false;p.reason=why;
            check(b.set(pkg,key,"optimized").equals("app_battery_restricted"));check(p.writes==writes);
            check(b.snapshot(17,pkg).getJSONArray("choices").length()==0);
        }
        p.mutable=true;p.shared=true;check(b.snapshot(18,pkg).getJSONArray("choices").length()==0);key=key(b,19);
        check(b.set(pkg,key,"optimized").equals("app_battery_restricted"));check(p.writes==writes);p.shared=false;
        p.availability="restricted";JSONObject state=b.snapshot(20,pkg);check(state.opt("key")==null&&state.opt("mode")==null);
        p.availability="missing";check(b.snapshot(21,pkg).opt("key")==null);p.availability="available";
        p.pending=true;key=key(b,22);check(b.set(pkg,key,"optimized").equals("app_battery_requested"));check(p.writes==++writes);p.pending=false;
        p.fail=true;key=key(b,23);check(b.set(pkg,key,"optimized").equals("app_battery_partial"));check(p.writes==++writes);
        check(b.set(pkg,key,"optimized").equals("app_battery_target_changed"));check(p.writes==writes);p.fail=false;
        p.throwWrite=true;key=key(b,24);try{b.set(pkg,key,"restricted");throw new AssertionError();}catch(IllegalStateException expected){checks++;}
        check(p.writes==++writes);check(b.set(pkg,key,"restricted").equals("app_battery_target_changed"));check(p.writes==writes);
        for(String bad:new String[]{"arbitrary","","Restricted","allowlist"})try{Mode.parse(bad);throw new AssertionError();}catch(IllegalArgumentException expected){checks++;}
        for(String bad:new String[]{"app","../pkg","com..pkg","with space","com.pkg$"})try{AppBatteryContract.packageName(bad);throw new AssertionError();}catch(IllegalArgumentException expected){checks++;}
        for(String bad:new String[]{"","a".repeat(63),"A".repeat(64),"g".repeat(64)})try{AppBatteryContract.key(bad);throw new AssertionError();}catch(IllegalArgumentException expected){checks++;}
        AppBatteryBackend unavailable=new AppBatteryBackend(null,()->now[0]);check(unavailable.snapshot(25,pkg).getString("availability").equals("unavailable"));
        check(unavailable.snapshot(26,pkg).getJSONArray("choices").length()==0);
        System.out.println("PASS app battery policy: "+checks);
    }
}
