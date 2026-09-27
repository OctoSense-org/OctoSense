package dev.makepad.octosense.appnetwork;

import java.security.SecureRandom;
import java.util.Arrays;
import java.util.Objects;
import java.util.function.LongSupplier;
import org.json.JSONArray;
import org.json.JSONObject;
import dev.makepad.octosense.appnetwork.AppNetworkContract.Field;

/** Native snapshots and a single-use, expiring, exact-incarnation policy choice. */
public final class AppNetworkBackend {
    public static final class State {
        public String availability="unavailable", identity="", label="";
        public String[] packages=new String[0];
        public int uid=-1, policy;
        public boolean writable, meteredMutable, dataSaver, lineage;
        public boolean equalsState(State s) {
            return s!=null&&availability.equals(s.availability)&&identity.equals(s.identity)
                &&uid==s.uid&&policy==s.policy&&writable==s.writable&&meteredMutable==s.meteredMutable
                &&dataSaver==s.dataSaver&&lineage==s.lineage&&Arrays.equals(packages,s.packages);
        }
        public Boolean value(Field f) {
            if(!"available".equals(availability))return null;
            switch(f) {
                case BACKGROUND:return (policy&(1|(lineage?0x50000:0)))==0;
                case UNRESTRICTED:return (policy&4)!=0&&(policy&(1|(lineage?0x50000:0)))==0;
                case NETWORK:return lineage?(policy&0x40000)==0:null;
                case WIFI:return lineage?(policy&0x48000)==0:null;
                case MOBILE:return lineage?(policy&0x50000)==0:null;
                case VPN:return lineage?(policy&0x60000)==0:null;
                default:throw new AssertionError();
            }
        }
        public boolean canSet(Field f) {
            if(!writable||value(f)==null)return false;
            switch(f) {
                case BACKGROUND:return meteredMutable&&(!lineage||(policy&0x50000)==0);
                case UNRESTRICTED:return meteredMutable&&(policy&(1|(lineage?0x50000:0)))==0;
                case NETWORK:return true;
                default:return (policy&0x40000)==0;
            }
        }
    }
    public interface Platform {
        State read(String packageName) throws Exception;
        void write(State observed,Field field,boolean enabled) throws Exception;
    }
    private final Platform platform;
    private final LongSupplier clock;
    private String packageName,key;
    private State observed;
    private long at=-1;
    public AppNetworkBackend(Platform platform,LongSupplier clock){this.platform=platform;this.clock=clock;}
    private State read(String pkg)throws Exception{return platform==null?new State():Objects.requireNonNull(platform.read(pkg));}
    private void invalidate(){packageName=null;key=null;observed=null;at=-1;}
    public synchronized JSONObject snapshot(long id,String pkg)throws Exception{
        AppNetworkContract.packageName(pkg);if(id<=0)throw new IllegalArgumentException("Positive request ID required");
        invalidate();State s=read(pkg);String token=null;
        if("available".equals(s.availability)){
            if(s.packages.length==0||s.packages.length>50)throw new IllegalStateException("Incomplete UID membership");
            byte[] nonce=new byte[32];new SecureRandom().nextBytes(nonce);StringBuilder hex=new StringBuilder();
            for(byte b:nonce)hex.append(String.format(java.util.Locale.ROOT,"%02x",b&255));token=hex.toString();
            packageName=pkg;key=token;observed=s;at=clock.getAsLong();
        }
        JSONArray rows=new JSONArray(),packages=new JSONArray();
        for(String p:s.packages)packages.put(p);
        for(Field f:Field.values())rows.put(new JSONObject().put("field",f.wire)
            .put("value",s.value(f)==null?JSONObject.NULL:s.value(f)).put("can_set",s.canSet(f)));
        return new JSONObject().put("schema",1).put("request_id",id).put("package",pkg)
            .put("availability",s.availability).put("key",token==null?JSONObject.NULL:token)
            .put("label",s.label).put("packages",packages).put("data_saver","available".equals(s.availability)?s.dataSaver:JSONObject.NULL)
            .put("controls",rows);
    }
    public synchronized String set(String pkg,String token,Field field,boolean enabled)throws Exception{
        AppNetworkContract.packageName(pkg);AppNetworkContract.key(token);Objects.requireNonNull(field);
        State before=observed;long now=clock.getAsLong();
        boolean fresh=pkg.equals(packageName)&&token.equals(key)&&at>=0&&now>=at&&now-at<=20000;
        // Even an attempted stale/denied operation consumes the lease.
        invalidate();if(!fresh)return "app_network_target_changed";
        State current=read(pkg);if(!before.equalsState(current))return "app_network_target_changed";
        if(!current.canSet(field))return "app_network_restricted";
        if(Boolean.valueOf(enabled).equals(current.value(field)))return "app_network_unchanged";
        try{platform.write(current,field,enabled);}
        catch(Exception partial){return "app_network_unconfirmed";}
        State after;
        try{after=read(pkg);}catch(Exception unreadable){return "app_network_unconfirmed";}
        if(!"available".equals(after.availability)||!current.identity.equals(after.identity))return "app_network_target_changed";
        int expected=policyAfter(current.policy,field,enabled);
        return after.policy==expected?"app_network_applied":"app_network_unconfirmed";
    }
    /** Mirrors native DataSaverBackend coupling, preserving every unrelated bit. */
    public static int policyAfter(int policy,Field field,boolean enabled){
        switch(field){
            case BACKGROUND:return ((enabled?policy&~1:policy|1)&~4);
            case UNRESTRICTED:return ((enabled?policy|4:policy&~4)&~1);
            default:int bit=field==Field.NETWORK?0x40000:field==Field.WIFI?0x8000:field==Field.MOBILE?0x10000:0x20000;
                return enabled?policy&~bit:policy|bit;
        }
    }
}
