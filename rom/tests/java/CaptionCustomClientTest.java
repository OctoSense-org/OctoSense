package dev.makepad.octosense;
import android.os.SystemClock;
import dev.makepad.octosense.agent.AgentPlatformClient;
import dev.makepad.octosense.controls.CaptionCustomContract;
import dev.makepad.octosense.controls.CaptionCustomSettings.Field;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;
import org.json.JSONArray;
import org.json.JSONObject;
public class CaptionCustomClientTest{
    static void check(boolean value){if(!value)throw new AssertionError();}
    static JSONObject state(long id,long visit)throws Exception{JSONArray rows=new JSONArray();for(Field f:Field.values()){JSONArray choices=new JSONArray();for(String choice:f.choices())choices.put(choice);rows.put(new JSONObject().put("id",f.id).put("value","observed").put("options",choices).put("availability","available"));}return CaptionCustomContract.envelope(id,visit,true,true,rows);}
    public static void main(String[] args)throws Exception{
        AgentPlatformClient agent=new AgentPlatformClient();AtomicBoolean foreground=new AtomicBoolean(true);CaptionCustomSettingsClient client=new CaptionCustomSettingsClient(agent,foreground::get);
        agent.state=state(1,1);client.snapshot(1,1);check(client.select(1,"caption_foreground_color","caption_swatch:48").equals("control_applied"));check(agent.writes==1);check(client.select(1,"caption_foreground_color","caption_swatch:48").equals("control_unavailable"));
        agent.state=state(2,1);client.snapshot(2,1);SystemClock.now=20001;check(client.select(1,"caption_foreground_color","caption_swatch:48").equals("control_unavailable"));SystemClock.now=0;
        agent.state=state(3,1);client.snapshot(3,1);foreground.set(false);check(client.select(1,"caption_foreground_color","caption_swatch:48").equals("control_unavailable"));foreground.set(true);
        agent.closeEntered=new CountDownLatch(1);agent.closeProceed=new CountDownLatch(1);client.retireInBackground();check(agent.closeEntered.await(2,TimeUnit.SECONDS));check(client.snapshot(4,1).getBoolean("scope_active")==false);agent.closeProceed.countDown();
        agent.state=state(5,2);client.snapshot(5,2);check(client.select(2,"caption_foreground_color","caption_swatch:48").equals("control_applied"));
        agent.state=state(6,3);agent.entered=new CountDownLatch(1);agent.proceed=new CountDownLatch(1);AtomicReference<JSONObject> reply=new AtomicReference<>();AtomicReference<Throwable> failure=new AtomicReference<>();Thread read=new Thread(()->{try{reply.set(client.snapshot(6,3));}catch(Throwable e){failure.set(e);}});read.start();check(agent.entered.await(2,TimeUnit.SECONDS));client.retireInBackground();agent.proceed.countDown();read.join(2000);check(!read.isAlive()&&failure.get()==null&&!reply.get().getBoolean("scope_active"));check(client.select(3,"caption_foreground_color","caption_swatch:48").equals("control_unavailable"));
        agent.entered=null;agent.state=state(7,4);client.snapshot(7,4);agent.state=state(8,4);agent.state.getJSONArray("controls").getJSONObject(1).getJSONArray("options").put("caption_swatch:raw");try{client.snapshot(8,4);throw new AssertionError();}catch(IllegalStateException|IllegalArgumentException expected){}check(client.select(4,"caption_foreground_color","caption_swatch:48").equals("control_unavailable"));check(agent.writes==2);
    }
}
