package dev.makepad.octosense.battery;

import java.util.function.LongSupplier;
import org.json.JSONArray;
import org.json.JSONObject;
import static dev.makepad.octosense.battery.AppBatteryContract.*;

/** One observed package policy at a time; native authority is rechecked before every write. */
public final class AppBatteryBackend {
    public static final long TTL_MS=20000;
    public static class State {
        public String packageName,label,incarnation,revision,availability="unavailable",reason="service_unavailable";
        public boolean sharedUid,canSet,allowlisted,preO;
        public Integer runAny,runIn;
        public String mode() {
            if(!"available".equals(availability)||runAny==null)return null;
            if(preO&&!java.util.Objects.equals(runIn,runAny))return "custom";
            if(runAny==1&&!allowlisted)return "restricted";
            if(runAny==0)return allowlisted?"unrestricted":"optimized";
            return "custom";
        }
        public boolean matches(Mode desired) {
            int expected=desired==Mode.RESTRICTED?1:0;
            return "available".equals(availability)&&runAny!=null&&runAny==expected
                &&allowlisted==(desired==Mode.UNRESTRICTED)&&(!preO||runIn!=null&&runIn==expected);
        }
        public boolean writable(){return "available".equals(availability)&&canSet&&!sharedUid&&runAny!=null&&incarnation!=null&&revision!=null;}
    }
    public enum Write { ACCEPTED, RESTRICTED, TARGET_CHANGED, PARTIAL, UNAVAILABLE }
    public interface Platform {
        State read(String packageName)throws Exception;
        Write apply(State observed,Mode mode)throws Exception;
    }
    private final Platform platform;private final LongSupplier clock;
    private State observed;private String observedKey;private long observedAt=-1;
    public AppBatteryBackend(Platform platform,LongSupplier clock){this.platform=platform;this.clock=clock;}
    private State read(String pkg)throws Exception {
        packageName(pkg);State state=platform==null?new State():platform.read(pkg);
        if(state==null)state=new State();state.packageName=pkg;return state;
    }
    public synchronized void invalidate(){observed=null;observedKey=null;observedAt=-1;}
    public synchronized JSONObject snapshot(long id,String pkg)throws Exception {
        if(id<=0)throw new IllegalArgumentException("Positive request ID required");
        State state=read(pkg);long now=clock.getAsLong();
        boolean same=observed!=null&&pkg.equals(observed.packageName)&&java.util.Objects.equals(state.incarnation,observed.incarnation)
            &&java.util.Objects.equals(state.revision,observed.revision)&&state.writable()==observed.writable();
        if(!same||observedAt<0||now<observedAt||now-observedAt>TTL_MS)observedKey=hash("app-battery",pkg,state.incarnation,state.revision,java.util.UUID.randomUUID().toString());
        observed=state;observedAt=now;
        JSONObject result=new JSONObject().put("schema",1).put("request_id",id).put("package",pkg)
            .put("availability",state.availability).put("reason",state.reason).put("shared_uid","available".equals(state.availability)&&state.sharedUid)
            .put("can_set",state.writable());
        JSONArray choices=new JSONArray();if(state.writable())for(Mode mode:Mode.values())choices.put(mode.wire);
        result.put("choices",choices);
        if("available".equals(state.availability)){result.put("key",observedKey);if(state.label!=null)result.put("label",state.label);if(state.mode()!=null)result.put("mode",state.mode());}
        return result;
    }
    public synchronized String set(String pkg,String target,String value)throws Exception {
        packageName(pkg);key(target);Mode desired=Mode.parse(value);long now=clock.getAsLong();
        State before=observed;
        if(before==null||!pkg.equals(before.packageName)||!target.equals(observedKey)||observedAt<0||now<observedAt||now-observedAt>TTL_MS)return "app_battery_target_changed";
        invalidate(); // claim before native work; failures never replay a multi-write
        State current=read(pkg);
        if(!current.writable())return "app_battery_restricted";
        if(!before.writable()||!java.util.Objects.equals(current.incarnation,before.incarnation)||!java.util.Objects.equals(current.revision,before.revision))return "app_battery_target_changed";
        Write written=platform.apply(current,desired);
        switch(written){
            case RESTRICTED:return "app_battery_restricted";
            case TARGET_CHANGED:return "app_battery_target_changed";
            case PARTIAL:return "app_battery_partial";
            case UNAVAILABLE:return "app_battery_unavailable";
            default:break;
        }
        State after=read(pkg);
        if(!java.util.Objects.equals(current.incarnation,after.incarnation))return "app_battery_target_changed";
        return after.matches(desired)?"app_battery_applied":"app_battery_requested";
    }
}
