package dev.makepad.octosense;

import android.app.Activity;
import android.content.Intent;
import android.content.pm.LauncherApps;
import android.content.pm.ShortcutInfo;
import android.graphics.drawable.Drawable;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.UserManager;
import android.view.Window;
import android.widget.Button;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.TextView;
import android.widget.Toast;
import java.io.File;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import org.json.JSONArray;
import org.json.JSONObject;

/** Public Android pin confirmation, independent of the Makepad renderer.
 * Platform queries, acceptance and placement storage use a bounded worker.
 * A submitted request runs once through recreation; it is never replayed.
 */
public final class ShortcutPinActivity extends Activity {
    private static final ThreadPoolExecutor WORKER=new ThreadPoolExecutor(1,1,0,TimeUnit.MILLISECONDS,
            new ArrayBlockingQueue<>(8),r -> new Thread(r,"OctoSenseShortcutPins"),new ThreadPoolExecutor.AbortPolicy());
    private static ShortcutPinActivity active;
    private static volatile String lastOutcome="";
    private final Handler main=new Handler(Looper.getMainLooper());
    private final String instance=java.util.UUID.randomUUID().toString();
    private volatile boolean destroyed,submitted;
    private boolean validation,ready;
    private volatile String outcome="loading";
    private String identity="",label="";
    private LauncherApps.PinItemRequest request;
    private TextView description;
    private ImageView icon;
    private Button add,cancel;
    private File journal;

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        validation=dev.makepad.octosense.validation.LauncherUiFixture.active();
        active=this;
        journal=new File(validation?getCacheDir():getFilesDir(),validation?"home-geometry-placements.json":"launcher-placements.json");
        // The previous Activity owns an already admitted worker operation.
        if(state!=null&&state.getBoolean("submitted")) {finish();return;}
        setTitle("Add shortcut");setFinishOnTouchOutside(true);
        LinearLayout content=new LinearLayout(this);content.setOrientation(LinearLayout.VERTICAL);
        int padding=Math.round(20*getResources().getDisplayMetrics().density);content.setPadding(padding,padding,padding,padding);
        icon=new ImageView(this);int size=Math.round(64*getResources().getDisplayMetrics().density);
        content.addView(icon,new LinearLayout.LayoutParams(size,size));
        description=new TextView(this);description.setText("Loading shortcut…");description.setTextSize(18);content.addView(description);
        LinearLayout buttons=new LinearLayout(this);
        cancel=new Button(this);cancel.setText("Cancel");cancel.setOnClickListener(v -> {outcome="cancelled";finish();});buttons.addView(cancel);
        add=new Button(this);add.setText("Add");add.setEnabled(false);add.setOnClickListener(v -> submit());buttons.addView(add);
        content.addView(buttons);setContentView(content);
        Intent incoming=new Intent(getIntent());
        if(!offer(() -> prepare(incoming))) fail("queue_full","Home is busy. Request the shortcut again.");
    }
    private boolean offer(Runnable operation) {
        try {WORKER.execute(operation);return true;} catch(RejectedExecutionException error) {return false;}
    }
    private void prepare(Intent incoming) {
        if(destroyed) return;
        try {
            LauncherApps launcher=getSystemService(LauncherApps.class);
            LauncherApps.PinItemRequest pin=launcher.getPinItemRequest(incoming);
            if(pin==null||pin.getRequestType()!=LauncherApps.PinItemRequest.REQUEST_TYPE_SHORTCUT||!pin.isValid()) {
                main.post(() -> fail("expired","This shortcut request has expired."));return;
            }
            ShortcutInfo shortcut=pin.getShortcutInfo();
            if(shortcut==null) throw new IllegalArgumentException("Missing shortcut");
            long serial=getSystemService(UserManager.class).getSerialNumberForUser(shortcut.getUserHandle());
            if(serial<0) throw new IllegalArgumentException("Unknown profile");
            String id="android-shortcut:"+serial+":"+shortcut.getPackage()+":"+shortcut.getId();
            String title=String.valueOf(shortcut.getShortLabel());
            Drawable preview=launcher.getShortcutBadgedIconDrawable(shortcut,getResources().getDisplayMetrics().densityDpi);
            if(preview==null) preview=getPackageManager().getApplicationIcon(shortcut.getPackage());
            Drawable drawable=preview;
            main.post(() -> {
                if(destroyed||isFinishing()) return;
                request=pin;identity=id;label=title;ready=true;outcome="ready";
                icon.setImageDrawable(drawable);description.setText(title+"\n"+shortcut.getPackage());add.setEnabled(true);
            });
        } catch(Exception error) {main.post(() -> fail("invalid","This shortcut is unavailable."));}
    }
    private void submit() {
        if(!ready||submitted||destroyed) return;
        submitted=true;outcome="queued";add.setEnabled(false);cancel.setEnabled(false);setFinishOnTouchOutside(false);
        if(!offer(this::accept)) {submitted=false;fail("queue_full","Home is busy. Request the shortcut again.");}
    }
    private void accept() {
        // Admission follows an explicit button press. Activity recreation or
        // destruction does not cancel or replay a possibly applied operation.
        try {
            LauncherPlacements placements=new LauncherPlacements(journal);
            if(!placements.isFavorite(identity)&&placements.snapshot().getJSONArray("favorites").length()>=128) {
                main.post(() -> fail("full","Home is full. Remove an item and try again."));return;
            }
            if(!request.isValid()||!request.accept()) {
                main.post(() -> fail("expired","This shortcut request has expired."));return;
            }
            outcome="pinned";lastOutcome=outcome;
            try {placements.favorite(identity,true);}
            catch(Exception error) {
                outcome="placement_failed";lastOutcome=outcome;
                main.post(() -> fail("placement_failed","The shortcut is pinned, but could not be placed on Home."));return;
            }
            main.post(() -> {if(!destroyed) finish();});
        } catch(SecurityException error) {main.post(() -> fail("denied","Android denied this shortcut request."));}
        catch(Exception error) {main.post(() -> fail("uncertain","Could not confirm whether the shortcut was added."));}
    }
    private void fail(String result,String message) {
        outcome=result;lastOutcome=result;
        if(!destroyed&&!isFinishing()) {Toast.makeText(this,message,Toast.LENGTH_LONG).show();finish();}
    }
    @Override protected void onSaveInstanceState(Bundle state) {state.putBoolean("submitted",submitted);super.onSaveInstanceState(state);}
    @Override protected void onPause() {
        super.onPause();
        if(!isChangingConfigurations()&&!submitted&&!isFinishing()) {outcome="cancelled";finish();}
    }
    @Override public void onDestroy() {destroyed=true;lastOutcome=outcome;if(active==this) active=null;super.onDestroy();}

    // App-owned validation hooks are inert outside the instrumentation fixture.
    public static Window validationWindow() {
        ShortcutPinActivity pin=active;
        return pin!=null&&pin.validation&&!pin.isFinishing()&&!pin.destroyed?pin.getWindow():null;
    }
    public static JSONObject validationState() throws org.json.JSONException {
        ShortcutPinActivity pin=active;
        JSONObject state=new JSONObject().put("showing",validationWindow()!=null&&pin.ready).put("outcome",pin==null?lastOutcome:pin.outcome);
        JSONArray buttons=new JSONArray();
        if(pin!=null&&pin.validation&&pin.ready&&!pin.destroyed) {
            state.put("identity",pin.identity).put("label",pin.label).put("instance",pin.instance);
            for(Button button:new Button[]{pin.add,pin.cancel}) {
                int[] position=new int[2];button.getLocationInWindow(position);
                buttons.put(new JSONObject().put("label",button.getText().toString()).put("x",position[0]+button.getWidth()/2)
                        .put("y",position[1]+button.getHeight()/2));
            }
        }
        return state.put("buttons",buttons);
    }
    public static boolean recreateValidation() {
        if(validationWindow()==null) return false;
        active.recreate();return true;
    }
    public static void closeValidation() {if(validationWindow()!=null) active.finish();}
}
