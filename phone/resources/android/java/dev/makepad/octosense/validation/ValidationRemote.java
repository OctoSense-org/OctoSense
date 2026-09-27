package dev.makepad.octosense.validation;

import android.graphics.Bitmap;
import android.graphics.Rect;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.view.MotionEvent;
import android.view.PixelCopy;
import android.view.SurfaceView;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import dev.makepad.android.MakepadActivity;
import java.io.ByteArrayOutputStream;
import java.io.Closeable;
import java.io.IOException;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.UUID;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import org.json.JSONObject;

/** Opt-in, token-protected loopback inspection of an owned validation activity.
 * Captures only this app's Window or Makepad SurfaceView. No OS/display capture,
 * external-app input, shell command, account data or arbitrary file route. */
public final class ValidationRemote implements Closeable {
    private static volatile ValidationRemote active;
    public static MakepadActivity ownedActivity() {ValidationRemote remote=active;return remote==null||remote.closed?null:remote.activity;}
    private final MakepadActivity activity;
    private final Runnable widgets,home;
    private final Handler main=new Handler(Looper.getMainLooper());
    private final ServerSocket server;
    private final String token=UUID.randomUUID().toString();
    private volatile boolean closed;
    private byte[] capturedImage;
    private String captureToken;
    private interface MainWork {void run() throws Exception;}
    public interface Status {JSONObject read() throws Exception;}
    private final Status widgetStatus;
    private final java.util.function.Supplier<Window> currentWindow;
    public ValidationRemote(MakepadActivity activity,Runnable widgets,Runnable home,Status widgetStatus,java.util.function.Supplier<Window> currentWindow) throws IOException {
        this.activity=activity;this.widgets=widgets;this.home=home;
        this.widgetStatus=widgetStatus;this.currentWindow=currentWindow;
        server=new ServerSocket(0,4,InetAddress.getByName("127.0.0.1"));
        active=this;
        Thread worker=new Thread(this::serve,"OctoSenseValidationRemote");worker.setDaemon(true);worker.start();
        android.util.Log.i("OctoSenseValidation","--remote pid="+android.os.Process.myPid()+" port="+server.getLocalPort()+" token="+token);
    }
    private void serve() {
        while(!closed) {
            try(Socket socket=server.accept()) {
                socket.setSoTimeout(3000);
                String request=line(socket);int bytes=request.length();
                for(int count=0;count<32;count++) {String header=line(socket);bytes+=header.length();if(bytes>8192) throw new IOException("Headers too large");if(header.isEmpty()) break;}
                String[] fields=request.split(" ");
                if(fields.length!=3||!fields[0].equals("GET")||!fields[1].startsWith("/"+token+"/")) {respond(socket,403,"text/plain","Forbidden".getBytes(StandardCharsets.UTF_8));continue;}
                android.net.Uri route=android.net.Uri.parse(fields[1].substring(token.length()+1));
                String path=route.getPath();byte[] result;String mime="application/json";
                if(path.equals("/capture/start")) {
                    capturedImage=capture("surface".equals(route.getQueryParameter("layer")));
                    captureToken=UUID.randomUUID().toString();
                    result=new JSONObject().put("capture",captureToken).put("bytes",capturedImage.length).toString().getBytes(StandardCharsets.UTF_8);
                } else if(path.equals("/capture/chunk")) {
                    if(capturedImage==null||!captureToken.equals(route.getQueryParameter("capture"))) throw new IllegalArgumentException("Unknown owned capture");
                    int offset=Integer.parseInt(route.getQueryParameter("offset")),length=Integer.parseInt(route.getQueryParameter("length"));
                    if(offset<0||length<1||length>16384||offset>capturedImage.length-length) throw new IllegalArgumentException("Capture chunk outside bounds");
                    result=java.util.Arrays.copyOfRange(capturedImage,offset,offset+length);mime="application/octet-stream";
                } else if(path.equals("/capture/release")) {
                    capturedImage=null;captureToken=null;result="{}".getBytes(StandardCharsets.UTF_8);
                } else if(path.equals("/g")||path.equals("/surface")||path.equals("/gq")) {
                    result=capture(path.equals("/surface"));mime="image/png";
                } else {
                    JSONObject value=new JSONObject();
                    onMain(() -> {
                        Window window=currentWindow.get();
                        if(window==null||!window.getDecorView().hasWindowFocus()) throw new IllegalStateException("Owned window is not focused");
                        switch(path) {
                            case "/": case "/status":
                                View root=activity.getApplicationOverlay();
                                int[] rootOffset=new int[2],surfaceOffset=new int[2];root.getLocationInWindow(rootOffset);
                                View drawable=surface(activity.getWindow().getDecorView());
                                if(drawable!=null) drawable.getLocationInWindow(surfaceOffset);
                                value.put("pid",android.os.Process.myPid()).put("width",root.getWidth()).put("height",root.getHeight())
                                    .put("window_capture_offset",new org.json.JSONArray().put(rootOffset[0]).put(rootOffset[1]))
                                    .put("surface_capture_offset",drawable==null?JSONObject.NULL:new org.json.JSONArray().put(surfaceOffset[0]).put(surfaceOffset[1]))
                                    .put("capture_window","/g").put("capture_makepad_surface","/surface").put("widgets",widgetStatus.read())
                                    .put("reply_fixture",ReplyUiFixture.state()).put("notification_flow",NotificationFlowFixture.state());break;
                            case "/flow/post": case "/flow/update": case "/flow/text": case "/flow/back": case "/flow/finish":
                                NotificationFlowFixture.command(route,activity);value.put("handled",true);break;
                            case "/reply/open": case "/reply/text": case "/reply/clear": case "/reply/result":
                            case "/reply/invalidate": case "/reply/disconnect": case "/reply/busy": case "/reply/close":
                                ReplyUiFixture.command(route);value.put("handled",true);break;
                            case "/notification/known": case "/notification/update": case "/notification/missing": case "/notification/clear":
                                NotificationUiFixture.command(route);value.put("handled",true);break;
                            case "/notification/probe":
                                String probeToken=route.getQueryParameter("token");
                                if(probeToken==null||probeToken.length()>64) throw new IllegalArgumentException("Invalid probe token");
                                value.put("queued",dev.makepad.android.MakepadNative.onAndroidIntegrationEvent("validation.notification_probe",new JSONObject().put("token",probeToken).toString()));break;
                            case "/launcher/probe":
                                String launcherToken=route.getQueryParameter("token"),identity=route.getQueryParameter("identity");
                                if(launcherToken==null||launcherToken.length()>64||identity==null||identity.length()>1024) throw new IllegalArgumentException("Invalid launcher probe");
                                value.put("queued",dev.makepad.android.MakepadNative.onAndroidIntegrationEvent("validation.launcher_probe",new JSONObject().put("token",launcherToken).put("identity",identity).toString()));break;
                            case "/shortcuts/cleanup":
                                value.put("queued",LauncherUiFixture.requestShortcutCleanup(route.getQueryParameter("owner")));break;
                            case "/page":
                                int page=Integer.parseInt(route.getQueryParameter("index"));if(page<0||page>32) throw new IllegalArgumentException("Invalid page");
                                boolean delivered=dev.makepad.android.MakepadNative.onAndroidIntegrationEvent("validation.navigation",new JSONObject().put("page",page).toString());
                                value.put("delivered",delivered);break;
                            case "/widgets": widgets.run();value.put("opened",true);break;
                            case "/home": home.run();value.put("closed_workspace",true);break;
                            case "/tap": case "/hold":
                                float x=number(route,"x"),y=number(route,"y");
                                View decor=window.getDecorView();
                                if(x<0||y<0||x>=decor.getWidth()||y>=decor.getHeight()) throw new IllegalArgumentException("Tap outside app");
                                long now=SystemClock.uptimeMillis();
                                touch(window,now,now,MotionEvent.ACTION_DOWN,x,y);
                                // Release the same owned window even if a hold
                                // opened a dialog, so its finger is not stranded.
                                main.postDelayed(() -> {if(!closed&&!activity.isDestroyed()) touch(window,now,SystemClock.uptimeMillis(),MotionEvent.ACTION_UP,x,y);},path.equals("/hold")?1000:60);
                                value.put("queued",true);break;
                            case "/swipe": {
                                float x1=number(route,"x1"),y1=number(route,"y1"),x2=number(route,"x2"),y2=number(route,"y2");
                                View target=window.getDecorView();
                                if(Math.min(x1,x2)<0||Math.min(y1,y2)<0||Math.max(x1,x2)>=target.getWidth()||Math.max(y1,y2)>=target.getHeight())
                                    throw new IllegalArgumentException("Swipe outside owned window");
                                long down=SystemClock.uptimeMillis();touch(window,down,down,MotionEvent.ACTION_DOWN,x1,y1);
                                for(int step=1;step<=20;step++) {final int index=step;main.postDelayed(() -> {
                                    if(!closed&&!activity.isDestroyed()) touch(window,down,SystemClock.uptimeMillis(),index==20?MotionEvent.ACTION_UP:MotionEvent.ACTION_MOVE,
                                            x1+(x2-x1)*index/20f,y1+(y2-y1)*index/20f);
                                },step*20);}
                                value.put("queued",true);break;
                            }
                            case "/recreate": main.post(activity::recreate);value.put("queued",true);break;
                            case "/pin/recreate": value.put("queued",dev.makepad.octosense.ShortcutPinActivity.recreateValidation());break;
                            case "/quit": case "/close": value.put("closed",true);break;
                            default:throw new IllegalArgumentException("Unknown app route");
                        }
                    });
                    result=value.toString().getBytes(StandardCharsets.UTF_8);
                }
                respond(socket,200,mime,result);
                if(path.equals("/gq")||path.equals("/quit")||path.equals("/close")) {
                    LauncherUiFixture.finish();
                    close();main.post(() -> {dev.makepad.octosense.ShortcutPinActivity.closeValidation();activity.finishAndRemoveTask();});
                }
            } catch(Exception e) {
                if(!closed) android.util.Log.w("OctoSenseValidation","Remote request failed: "+e.getClass().getSimpleName());
            }
        }
    }
    private static float number(android.net.Uri uri,String key) {
        float value=Float.parseFloat(uri.getQueryParameter(key));if(!Float.isFinite(value)) throw new IllegalArgumentException("Invalid coordinate");return value;
    }
    private void touch(Window window,long down,long time,int action,float x,float y) {
        MotionEvent event=MotionEvent.obtain(down,time,action,x,y,0);try {window.getDecorView().dispatchTouchEvent(event);} finally {event.recycle();}
    }
    private static String line(Socket socket) throws IOException {
        ByteArrayOutputStream bytes=new ByteArrayOutputStream();int value;
        while((value=socket.getInputStream().read())!=-1) {
            if(value=='\n') return bytes.toString("UTF-8").replace("\r","");
            if(bytes.size()>=4096) throw new IOException("Line too large");bytes.write(value);
        }
        throw new IOException("Missing HTTP line");
    }
    private static void respond(Socket socket,int status,String mime,byte[] bytes) throws IOException {
        String header="HTTP/1.1 "+status+(status==200?" OK":" Forbidden")+"\r\nContent-Type: "+mime+"\r\nContent-Length: "+bytes.length+"\r\nConnection: close\r\n\r\n";
        socket.getOutputStream().write(header.getBytes(StandardCharsets.US_ASCII));socket.getOutputStream().write(bytes);socket.getOutputStream().flush();
        // Let the client consume Content-Length and close first. Immediate
        // server close can race large responses through the ADB forward.
        // This wait is bounded by the socket timeout and runs on this worker.
        try {socket.getInputStream().read();} catch(java.net.SocketTimeoutException ignored) {}
    }
    private void onMain(MainWork work) throws Exception {
        CountDownLatch done=new CountDownLatch(1);Exception[] error=new Exception[1];
        java.util.concurrent.atomic.AtomicBoolean cancelled=new java.util.concurrent.atomic.AtomicBoolean();
        main.post(() -> {try {if(closed||cancelled.get()) throw new IOException("Remote closed");work.run();} catch(Exception e) {error[0]=e;} finally {done.countDown();}});
        if(!done.await(3,TimeUnit.SECONDS)) {cancelled.set(true);throw new IOException("Activity timeout");}if(error[0]!=null) throw error[0];
    }
    private byte[] capture(boolean surface) throws Exception {
        CountDownLatch done=new CountDownLatch(1);Bitmap[] bitmap=new Bitmap[1];int[] status={PixelCopy.ERROR_UNKNOWN};
        // PixelCopy may complete after our timeout. Exactly one side owns the
        // bitmap after completion; never recycle a copy still in progress.
        java.util.concurrent.atomic.AtomicInteger ownership=new java.util.concurrent.atomic.AtomicInteger();
        try {
          onMain(() -> {
            Window window=surface?activity.getWindow():currentWindow.get();
            if(window==null||!window.getDecorView().hasWindowFocus()) throw new IllegalStateException("Owned window is not focused");
            View root=surface?surface(window.getDecorView()):window==activity.getWindow()?activity.getApplicationOverlay():window.getDecorView();
            if(root==null||root.getWidth()<=0||root.getHeight()<=0||(long)root.getWidth()*root.getHeight()>16_000_000) throw new IOException("Drawable unavailable");
            bitmap[0]=Bitmap.createBitmap(root.getWidth(),root.getHeight(),Bitmap.Config.ARGB_8888);
            PixelCopy.OnPixelCopyFinishedListener finish=result -> {
                status[0]=result;if(!ownership.compareAndSet(0,1)) bitmap[0].recycle();done.countDown();
            };
            try {
                if(surface) PixelCopy.request((SurfaceView)root,bitmap[0],finish,main);
                else {
                    int[] location=new int[2];root.getLocationInWindow(location);
                    PixelCopy.request(window,new Rect(location[0],location[1],location[0]+root.getWidth(),location[1]+root.getHeight()),bitmap[0],finish,main);
                }
            } catch(RuntimeException e) {bitmap[0].recycle();throw e;}
          });
            if(!done.await(5,TimeUnit.SECONDS)) throw new IOException("PixelCopy timeout");
            if(status[0]!=PixelCopy.SUCCESS) throw new IOException("PixelCopy result "+status[0]);
            ByteArrayOutputStream bytes=new ByteArrayOutputStream();bitmap[0].compress(Bitmap.CompressFormat.PNG,100,bytes);return bytes.toByteArray();
        } finally {if(ownership.getAndSet(2)==1) bitmap[0].recycle();}
    }
    private static SurfaceView surface(View view) {
        if(view instanceof SurfaceView) return (SurfaceView)view;
        if(view instanceof ViewGroup) for(int index=0;index<((ViewGroup)view).getChildCount();index++) {
            SurfaceView found=surface(((ViewGroup)view).getChildAt(index));if(found!=null) return found;
        }
        return null;
    }
    @Override public void close() throws IOException {closed=true;capturedImage=null;captureToken=null;if(active==this) active=null;server.close();}
}
