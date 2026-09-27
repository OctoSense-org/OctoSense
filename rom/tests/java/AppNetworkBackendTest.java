import dev.makepad.octosense.appnetwork.AppNetworkBackend;
import dev.makepad.octosense.appnetwork.AppNetworkContract;
import dev.makepad.octosense.appnetwork.AppNetworkContract.Field;
import org.json.JSONObject;

public final class AppNetworkBackendTest {
    static int checks;
    static void check(boolean ok){checks++;if(!ok)throw new AssertionError("Check "+checks);}
    static final class Platform implements AppNetworkBackend.Platform {
        int policy=0x800000, writes;boolean mutable=true,metered=true,lineage,partial;String incarnation="install-A";
        public AppNetworkBackend.State read(String pkg){
            AppNetworkBackend.State s=new AppNetworkBackend.State();s.availability="available";s.label="Fixture";
            s.uid=10123;s.packages=new String[]{pkg};s.identity=incarnation;s.policy=policy;s.writable=mutable;s.meteredMutable=metered;s.lineage=lineage;return s;
        }
        public void write(AppNetworkBackend.State s,Field f,boolean enabled){
            writes++;policy=AppNetworkBackend.policyAfter(policy,f,enabled);if(partial)throw new IllegalStateException("Second native operation failed");
        }
    }
    public static void main(String[] args)throws Exception{
        Platform p=new Platform();long[] now={100};AppNetworkBackend b=new AppNetworkBackend(p,()->now[0]);String pkg="fixture.network";
        JSONObject v=b.snapshot(1,pkg);String key=v.getString("key");check(v.getJSONArray("controls").length()==6);
        check(b.set(pkg,key,Field.BACKGROUND,false).equals("app_network_applied"));check(p.policy==0x800001);check(p.writes==1);
        check(b.set(pkg,key,Field.BACKGROUND,true).equals("app_network_target_changed"));check(p.writes==1);
        key=b.snapshot(2,pkg).getString("key");check(b.set(pkg,key,Field.UNRESTRICTED,true).equals("app_network_restricted"));check(p.writes==1);
        key=b.snapshot(3,pkg).getString("key");check(b.set(pkg,key,Field.BACKGROUND,true).equals("app_network_applied"));check(p.policy==0x800000);
        key=b.snapshot(4,pkg).getString("key");check(b.set(pkg,key,Field.UNRESTRICTED,true).equals("app_network_applied"));check(p.policy==0x800004);
        key=b.snapshot(5,pkg).getString("key");check(b.set(pkg,key,Field.BACKGROUND,false).equals("app_network_applied"));check(p.policy==0x800001);
        key=b.snapshot(6,pkg).getString("key");p.incarnation="install-B";check(b.set(pkg,key,Field.BACKGROUND,true).equals("app_network_target_changed"));
        key=b.snapshot(7,pkg).getString("key");p.policy=0;check(b.set(pkg,key,Field.BACKGROUND,false).equals("app_network_target_changed"));
        key=b.snapshot(8,pkg).getString("key");now[0]+=20001;check(b.set(pkg,key,Field.BACKGROUND,false).equals("app_network_target_changed"));
        key=b.snapshot(9,pkg).getString("key");p.mutable=false;check(b.set(pkg,key,Field.BACKGROUND,false).equals("app_network_target_changed"));
        key=b.snapshot(10,pkg).getString("key");check(b.set(pkg,key,Field.BACKGROUND,false).equals("app_network_restricted"));p.mutable=true;
        key=b.snapshot(11,pkg).getString("key");check(b.set(pkg,key,Field.WIFI,false).equals("app_network_restricted"));
        p.lineage=true;for(Field field:new Field[]{Field.NETWORK,Field.WIFI,Field.MOBILE,Field.VPN}){
            key=b.snapshot(12,pkg).getString("key");check(b.set(pkg,key,field,false).equals("app_network_applied"));
            check(Boolean.FALSE.equals(p.read(pkg).value(field)));key=b.snapshot(13,pkg).getString("key");
            check(b.set(pkg,key,field,true).equals("app_network_applied"));check(p.policy==0);
        }
        p.policy=0x40000;AppNetworkBackend.State state=p.read(pkg);check(!state.canSet(Field.BACKGROUND));check(!state.canSet(Field.UNRESTRICTED));check(!state.canSet(Field.WIFI));check(state.canSet(Field.NETWORK));
        p.policy=0;p.metered=false;key=b.snapshot(14,pkg).getString("key");check(b.set(pkg,key,Field.BACKGROUND,false).equals("app_network_restricted"));p.metered=true;
        key=b.snapshot(15,pkg).getString("key");check(b.set(pkg,key,Field.BACKGROUND,true).equals("app_network_unchanged"));
        p.partial=true;key=b.snapshot(16,pkg).getString("key");int writes=p.writes;
        check(b.set(pkg,key,Field.BACKGROUND,false).equals("app_network_unconfirmed"));check(p.writes==writes+1);check(p.policy==1);
        check(b.set(pkg,key,Field.BACKGROUND,true).equals("app_network_target_changed"));check(p.writes==writes+1);
        for(String bad:new String[]{"../app","with space","com..x","app/other"}){try{AppNetworkContract.packageName(bad);throw new AssertionError();}catch(IllegalArgumentException expected){checks++;}}
        for(String bad:new String[]{"clear","uid_policy","","BACKGROUND"}){try{Field.parse(bad);throw new AssertionError();}catch(IllegalArgumentException expected){checks++;}}
        AppNetworkBackend unavailable=new AppNetworkBackend(null,()->now[0]);check(unavailable.snapshot(17,pkg).getString("availability").equals("unavailable"));
        System.out.println("PASS app network policy: "+checks);
    }
}
