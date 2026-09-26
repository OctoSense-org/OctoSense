import dev.makepad.octosense.dnd.DndSettingsBackend;
import dev.makepad.octosense.dnd.DndSettingsContract;
import static dev.makepad.octosense.dnd.DndSettingsContract.*;
import org.json.JSONObject;

public final class DndSettingsBackendTest {
    private static long time=100;
    private static void check(boolean value,String message){if(!value)throw new AssertionError(message);}
    private static void invalid(Runnable run){try{run.run();throw new AssertionError("accepted invalid input");}catch(IllegalArgumentException expected){}}
    private static class Platform implements DndSettingsBackend.Platform {
        boolean owner=true,write=true,defer;int calls;String revision="first";
        Policy policy=new Policy(0x40000000|8|32,2,1,0x7f,3,2);
        DndSettingsBackend.Rule row=new DndSettingsBackend.Rule();
        Platform(){row.target=hash("native-id","creation-1");row.name="Night";row.revision="rule-v1";row.enabled=true;row.editable=true;row.deletable=true;row.schedule=new Schedule("Night",new int[]{2,3},1380,420,true,true);}
        public DndSettingsBackend.State read(){DndSettingsBackend.State s=new DndSettingsBackend.State();s.available=owner;s.writable=owner&&write;s.policyWritable=write;s.canCreate=write;s.policy=owner?policy:null;s.effectivePolicy=s.policy;s.revision=revision;if(owner&&row!=null)s.rules.add(row);return s;}
        public boolean policy(DndSettingsBackend.State before,Policy value){calls++;if(!defer)policy=value;return true;}
        public boolean schedule(DndSettingsBackend.State before,DndSettingsBackend.Rule target,Schedule value){calls++;if(!defer){row.name=value.name;row.schedule=value;row.enabled=value.enabled;row.revision="changed";}return true;}
        public boolean enabled(DndSettingsBackend.State before,DndSettingsBackend.Rule target,boolean value){calls++;if(!defer)row.enabled=value;return true;}
        public boolean delete(DndSettingsBackend.State before,DndSettingsBackend.Rule target){calls++;if(!defer)row=null;return true;}
    }
    public static void main(String[] args)throws Exception{
        invalid(()->Field.parse("arbitrary_key"));invalid(()->Field.CALLS.value("on"));invalid(()->Field.MEDIA.value("contacts"));
        invalid(()->offset(20,null));invalid(()->offset(1,null));invalid(()->minute(1440));invalid(()->days(new int[]{1,1}));invalid(()->days(new int[]{0}));invalid(()->name("name\n"));
        Policy original=new Policy(0x40000000|8|4,2,1,0x7e,0x405,2);
        Policy changed=original.change(Field.MEDIA,"on");check(changed.categories==(original.categories|64),"unknown category lost");
        check(changed.calls==2&&changed.messages==1&&changed.effects==0x7e&&changed.state==0x405&&changed.conversations==2,"unrelated policy field changed");
        changed=original.change(Field.CALLS,"none");check(changed.calls==2&&(changed.categories&8)==0,"None must preserve sender preference");
        check(new Policy(8,99,1,0,0,0).value(Field.CALLS)==null,"unknown sender invented");
        Platform platform=new Platform();DndSettingsBackend backend=new DndSettingsBackend(platform,()->time);
        JSONObject first=backend.snapshot(1,0,null);String key=first.getString("key");
        check(key.equals(backend.snapshot(2,0,null).getString("key")),"unchanged poll invalidates draft");
        check(backend.policy(key,Field.MEDIA,"on").equals("dnd_applied"),"write/readback failed");
        check(platform.policy.effects==0x7f&&platform.policy.state==3,"backend discarded untouched fields");
        check(backend.policy(key,Field.MEDIA,"off").equals("dnd_target_changed"),"claimed key replayed");
        key=backend.snapshot(3,0,null).getString("key");int calls=platform.calls;platform.revision="external";
        check(backend.policy(key,Field.MEDIA,"off").equals("dnd_target_changed")&&platform.calls==calls,"external changes not rejected");
        key=backend.snapshot(4,0,null).getString("key");platform.owner=false;
        check(backend.policy(key,Field.MEDIA,"off").equals("dnd_target_changed")&&platform.calls==calls,"owner switch wrote policy");platform.owner=true;
        key=backend.snapshot(5,0,null).getString("key");platform.write=false;
        check(backend.policy(key,Field.MEDIA,"off").equals("dnd_target_changed"),"revoked capability used");platform.write=true;
        key=backend.snapshot(6,0,null).getString("key");time+=20001;
        check(backend.policy(key,Field.MEDIA,"off").equals("dnd_target_changed"),"expired key used");
        String replacement=backend.snapshot(7,0,null).getString("key");check(!replacement.equals(key),"expired lease identity reused");
        platform.defer=true;check(backend.policy(replacement,Field.MEDIA,"off").equals("dnd_requested"),"unobserved write called applied");platform.defer=false;
        key=backend.snapshot(8,0,null).getString("key");check(backend.enabled(key,hash("forged"),false).equals("dnd_unavailable"),"forged rule accepted");
        key=backend.snapshot(9,0,null).getString("key");String target=platform.row.target;
        check(backend.schedule(key,target,new Schedule("Reviewed",new int[]{7,1},0,0,false,false)).equals("dnd_applied"),"reviewed overnight/equal schedule failed");
        check(platform.row.schedule.days[0]==1&&platform.row.schedule.start==0&&platform.row.schedule.end==0,"schedule normalization wrong");
        key=backend.snapshot(10,0,null).getString("key");check(backend.delete(key,target).equals("dnd_applied"),"delete missing readback");
        String generation=first.getString("generation");JSONObject stale=backend.snapshot(11,20,generation);check(stale.getBoolean("stale")&&stale.getInt("offset")==0,"changed catalog didn't reset");
        System.out.println("DND contract/leases/copy/readback PASS");
    }
}
