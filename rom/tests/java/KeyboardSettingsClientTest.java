package dev.makepad.octosense;

import android.app.PendingIntent;
import android.os.SystemClock;
import dev.makepad.octosense.keyboards.KeyboardJson;
import dev.makepad.octosense.keyboards.KeyboardPolicy;
import java.util.ArrayList;
import java.util.List;
import org.json.JSONObject;

public final class KeyboardSettingsClientTest {
    static int checks;static void check(boolean value){if(!value)throw new AssertionError("Keyboard client check "+checks);checks++;}
    public static void main(String[] args)throws Exception{
        boolean[] foreground={true};int[] prepared={0};
        KeyboardPolicy backend=new KeyboardPolicy(()->{
            List<KeyboardPolicy.Method> rows=new ArrayList<>();for(int i=0;i<25;i++)rows.add(new KeyboardPolicy.Method("provider"+i+"/.Ime","install-A","Provider "+i,"provider"+i,"English","",i==0,i==0,i==0,true,false,i!=0,false,true,i==0));
            return new KeyboardPolicy.State("state-A","available","none","provider0/.Ime",true,rows);
        },()->SystemClock.now);
        KeyboardSettingsClient client=new KeyboardSettingsClient(new KeyboardSettingsClient.Bridge(){
            public JSONObject snapshot(long id,String query,int offset){return KeyboardJson.page(id,backend.snapshot(query,offset));}
            public PendingIntent prepare(long id,String key,String target,String operation){prepared[0]++;return new PendingIntent();}
        },()->foreground[0]);
        JSONObject page=client.snapshot(1,"",0);String key=page.getString("key"),first=page.getJSONArray("rows").getJSONObject(1).getString("target");
        check(client.prepare(2,key,first,"disable")==null&&prepared[0]==0);
        check(client.prepare(3,key,first,"enable")!=null&&prepared[0]==1);check(client.prepare(4,key,first,"enable")==null&&prepared[0]==1);
        page=client.snapshot(5,"",0);key=page.getString("key");first=page.getJSONArray("rows").getJSONObject(1).getString("target");
        client.snapshot(6,"",20);check(client.prepare(7,key,first,"enable")==null&&prepared[0]==1);
        page=client.snapshot(8,"",0);key=page.getString("key");first=page.getJSONArray("rows").getJSONObject(1).getString("target");foreground[0]=false;
        check(client.prepare(9,key,first,"enable")==null&&prepared[0]==1);check(client.snapshot(10,"",0).getString("availability").equals("restricted"));foreground[0]=true;
        page=client.snapshot(11,"",0);key=page.getString("key");SystemClock.now+=30001;
        check(client.prepare(12,key,"","choose_default")==null&&prepared[0]==1);
        page=client.snapshot(13,"",0);key=page.getString("key");check(client.prepare(14,key,"","choose_default")!=null&&prepared[0]==2);
        page=client.snapshot(15,"",0);key=page.getString("key");client.invalidate();check(client.prepare(16,key,"","choose_default")==null&&prepared[0]==2);
        System.out.println("PASS keyboard foreground client "+checks);
    }
}
