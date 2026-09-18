package dev.makepad.octosense.bridge.validation;

import android.app.Activity;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;
import dev.makepad.octosense.bridge.R;
import android.net.LocalServerSocket;
import android.net.LocalSocket;
import java.nio.charset.StandardCharsets;
import java.util.UUID;

/** Disposable launch target in the opt-in validation APK. Its local remote
 * reports only this activity's lifecycle and closes only this owned task. */
public final class LauncherFixtureActivity extends Activity {
    private final Handler main=new Handler(Looper.getMainLooper());
    private final String token=UUID.randomUUID().toString();
    private LocalServerSocket server;
    private volatile boolean resumed,closed;
    private volatile String pinId="",launchedShortcut="",pinError="";
    private String shortcutOperation="",operationError="",launchedRevision="";
    private volatile boolean pinRequested,pinAccepted;
    private android.content.BroadcastReceiver pinResult;
    private int task;
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);task=getTaskId();
        launchedShortcut=getIntent().getStringExtra("octosense.validation.shortcut");
        if(launchedShortcut==null) launchedShortcut="";
        if(launchedShortcut.matches("octosense-validation-pin-[a-f0-9-]{36}-accept")) pinId=launchedShortcut;
        launchedRevision=getIntent().getStringExtra("octosense.validation.revision");if(launchedRevision==null) launchedRevision="";
        pinResult=new android.content.BroadcastReceiver() {
            @Override public void onReceive(android.content.Context context,android.content.Intent intent) {pinAccepted=true;}
        };
        android.content.IntentFilter filter=new android.content.IntentFilter(getPackageName()+".PIN_RESULT."+token);
        if(android.os.Build.VERSION.SDK_INT>=33) registerReceiver(pinResult,filter,RECEIVER_NOT_EXPORTED);
        else registerReceiver(pinResult,filter);
        LinearLayout root=new LinearLayout(this);root.setOrientation(LinearLayout.VERTICAL);root.setPadding(32,96,32,32);
        TextView title=new TextView(this);title.setText(R.string.launcher_fixture_title);title.setTextSize(24);root.addView(title);
        Button done=new Button(this);done.setText(R.string.launcher_fixture_return);done.setOnClickListener(view -> finishAndRemoveTask());root.addView(done);setContentView(root);
        try {
            String address="octosense-launcher-fixture-"+token;
            server=new LocalServerSocket(address);
            Thread worker=new Thread(this::serve,"OctoSenseLauncherFixture");worker.setDaemon(true);worker.start();
            android.util.Log.i("OctoSenseLauncherFixture","--remote pid="+android.os.Process.myPid()+" socket="+address+" token="+token);
        } catch(java.io.IOException e) {finishAndRemoveTask();}
        main.postDelayed(this::finishAndRemoveTask,180_000);
    }
    @Override public void onResume() {super.onResume();resumed=true;}
    @Override public void onPause() {resumed=false;super.onPause();}
    private void serve() {
        while(!closed) try(LocalSocket socket=server.accept()) {
            socket.setSoTimeout(2000);
            String line=line(socket);int bytes=line.length();
            for(int count=0;count<32;count++) {String header=line(socket);bytes+=header.length();if(bytes>8192) throw new java.io.IOException("Headers too large");if(header.isEmpty()) break;}
            boolean status=("GET /"+token+"/status HTTP/1.1").equals(line);
            boolean quit=("GET /"+token+"/close HTTP/1.1").equals(line);
            boolean background=("GET /"+token+"/background HTTP/1.1").equals(line);
            boolean change=line.startsWith("GET /"+token+"/shortcut?op=")&&line.endsWith(" HTTP/1.1");
            if(change) {
                String operation=android.net.Uri.parse(line.split(" ")[1]).getQueryParameter("op");
                if(!java.util.Arrays.asList("update","disable","enable").contains(operation)) change=false;
                else changeShortcut(operation);
            }
            boolean pin=line.startsWith("GET /"+token+"/pin?case=")&&line.endsWith(" HTTP/1.1");
            if(pin) {
                String scenario=android.net.Uri.parse(line.split(" ")[1]).getQueryParameter("case");
                String owner=android.net.Uri.parse(line.split(" ")[1]).getQueryParameter("owner");
                if(!java.util.Arrays.asList("cancel","recreate","accept").contains(scenario)||owner==null||!owner.matches("[a-f0-9-]{36}")) pin=false;
                else {
                    java.util.concurrent.CountDownLatch done=new java.util.concurrent.CountDownLatch(1);
                    main.post(() -> {try {requestPin(scenario,owner);} finally {done.countDown();}});
                    try {if(!done.await(3,java.util.concurrent.TimeUnit.SECONDS)) throw new java.io.IOException("Pin timeout");}
                    catch(InterruptedException error) {Thread.currentThread().interrupt();throw new java.io.IOException(error);}
                }
            }
            android.util.Log.i("OctoSenseLauncherFixture","request="+(status?"status":quit?"close":pin?"pin":"rejected")+" task="+task);
            boolean persisted=false,enabled=false;String shortcutLabel="";long changed=0;
            if(!pinId.isEmpty()) for(android.content.pm.ShortcutInfo shortcut:getSystemService(android.content.pm.ShortcutManager.class).getPinnedShortcuts())
                if(pinId.equals(shortcut.getId())) {persisted=true;enabled=shortcut.isEnabled();shortcutLabel=String.valueOf(shortcut.getShortLabel());changed=shortcut.getLastChangedTimestamp();}
            boolean allowed=status||quit||pin||background||change;
            String value=allowed?"{\"pid\":"+android.os.Process.myPid()+",\"task\":"+task+",\"resumed\":"+resumed+",\"closed\":"+quit
                    +",\"shortcut_id\":\""+pinId+"\",\"pin_requested\":"+pinRequested+",\"pin_accepted\":"+pinAccepted
                    +",\"pin_persisted\":"+persisted
                    +",\"shortcut_enabled\":"+enabled+",\"shortcut_label\":"+org.json.JSONObject.quote(shortcutLabel)+",\"shortcut_changed\":"+changed
                    +",\"shortcut_operation\":"+org.json.JSONObject.quote(shortcutOperation)+",\"operation_error\":"+org.json.JSONObject.quote(operationError)
                    +",\"launched_revision\":"+org.json.JSONObject.quote(launchedRevision)
                    +",\"pin_error\":\""+pinError+"\",\"launched_shortcut\":"+org.json.JSONObject.quote(launchedShortcut)+"}":"{}";
            byte[] body=value.getBytes(StandardCharsets.UTF_8);
            String header="HTTP/1.1 "+(allowed?"200 OK":"403 Forbidden")+"\r\nContent-Type: application/json\r\nContent-Length: "+body.length+"\r\nConnection: close\r\n\r\n";
            socket.getOutputStream().write(header.getBytes(StandardCharsets.US_ASCII));socket.getOutputStream().write(body);socket.getOutputStream().flush();
            if(quit) {main.post(this::finishAndRemoveTask);return;}
            if(background) main.post(() -> moveTaskToBack(true));
        } catch(java.io.IOException e) {if(!closed) android.util.Log.w("OctoSenseLauncherFixture","Remote request failed");}
    }
    private void changeShortcut(String operation) {
        // This socket's worker may mutate only this fixture's accepted pin.
        // It never touches a real publisher's shortcuts or the main thread.
        shortcutOperation=operation;operationError="";
        try {
            if(!pinId.matches("octosense-validation-pin-[a-f0-9-]{36}-accept")) throw new IllegalArgumentException("No owned pin");
            android.content.pm.ShortcutManager manager=getSystemService(android.content.pm.ShortcutManager.class);
            boolean owned=false;for(android.content.pm.ShortcutInfo shortcut:manager.getPinnedShortcuts()) if(pinId.equals(shortcut.getId())) owned=true;
            if(!owned) throw new IllegalStateException("Owned pin missing");
            java.util.List<String> ids=java.util.Collections.singletonList(pinId);
            if(operation.equals("disable")) manager.disableShortcuts(ids,"This test shortcut was retired by its app.");
            else if(operation.equals("enable")) manager.enableShortcuts(ids);
            else {
                android.content.Intent launch=new android.content.Intent(this,LauncherFixtureActivity.class).setAction(android.content.Intent.ACTION_VIEW)
                        .putExtra("octosense.validation.shortcut",pinId).putExtra("octosense.validation.revision","updated");
                android.content.pm.ShortcutInfo shortcut=new android.content.pm.ShortcutInfo.Builder(this,pinId)
                        .setShortLabel("Pin updated").setIntent(launch)
                        .setIcon(android.graphics.drawable.Icon.createWithResource(this,android.R.drawable.ic_menu_camera)).build();
                if(!manager.updateShortcuts(java.util.Collections.singletonList(shortcut))) throw new IllegalStateException("Rate limited");
            }
        } catch(Exception error) {operationError=error.getClass().getSimpleName();}
    }
    private void requestPin(String scenario,String owner) {
        pinError="";pinRequested=false;pinAccepted=false;
        if(!resumed) {pinError="not_foreground";return;}
        pinId="octosense-validation-pin-"+owner+"-"+scenario;
        try {
            android.content.pm.ShortcutManager manager=getSystemService(android.content.pm.ShortcutManager.class);
            android.content.Intent launch=new android.content.Intent(this,LauncherFixtureActivity.class)
                    .setAction(android.content.Intent.ACTION_VIEW).putExtra("octosense.validation.shortcut",pinId);
            android.content.pm.ShortcutInfo shortcut=new android.content.pm.ShortcutInfo.Builder(this,pinId)
                    .setShortLabel("Pin check "+scenario).setLongLabel("OctoSense shortcut validation")
                    .setActivity(new android.content.ComponentName(this,LauncherFixtureActivity.class))
                    .setIntent(launch).setIcon(android.graphics.drawable.Icon.createWithResource(this,android.R.drawable.ic_menu_compass)).build();
            android.app.PendingIntent callback=android.app.PendingIntent.getBroadcast(this,0,
                    new android.content.Intent(getPackageName()+".PIN_RESULT."+token).setPackage(getPackageName()),
                    android.app.PendingIntent.FLAG_IMMUTABLE|android.app.PendingIntent.FLAG_UPDATE_CURRENT);
            pinRequested=manager.requestPinShortcut(shortcut,callback.getIntentSender());
        } catch(Exception error) {pinError=error.getClass().getSimpleName();}
    }
    private static String line(LocalSocket socket) throws java.io.IOException {
        java.io.ByteArrayOutputStream bytes=new java.io.ByteArrayOutputStream();int value;
        while((value=socket.getInputStream().read())!=-1) {
            if(value=='\n') return bytes.toString("US-ASCII").replace("\r","");
            if(bytes.size()>=512) throw new java.io.IOException("Line too large");bytes.write(value);
        }
        throw new java.io.IOException("Missing HTTP line");
    }
    @Override public void onDestroy() {
        closed=true;main.removeCallbacksAndMessages(null);
        if(server!=null) try {server.close();} catch(java.io.IOException ignored) {}
        if(pinResult!=null) unregisterReceiver(pinResult);
        android.util.Log.i("OctoSenseLauncherFixture","closed task="+task);
        super.onDestroy();
    }
}
