package dev.makepad.octosense;

import android.app.Activity;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageInfo;
import android.content.pm.PackageInstaller;
import android.content.pm.PackageManager;
import android.content.pm.Signature;
import android.net.Uri;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.provider.Settings;
import java.io.File;
import java.io.IOException;
import java.io.OutputStream;
import java.util.UUID;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.BiConsumer;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** Home's own updater. Only the trusted native updater UI sends this private
 * integration channel; this is not an OctoScript capability or an agent tool.
 * It never contacts the ROM updater and never installs Bridge or another app. */
final class HomeUpdaterClient {
    private final Activity activity;
    private final BiConsumer<String,JSONObject> emit;
    private final BooleanSupplier foreground;
    private final Handler main=new Handler(Looper.getMainLooper());
    private final AtomicBoolean preparing=new AtomicBoolean();
    private final AtomicLong generation=new AtomicLong();
    private final Object stagingLock=new Object();
    private int stagedSession=-1;
    private long stagedGeneration=-1;
    private final ThreadPoolExecutor worker=new ThreadPoolExecutor(1,1,0,TimeUnit.MILLISECONDS,
        new ArrayBlockingQueue<>(8),r -> new Thread(r,"HomeUpdater"),new ThreadPoolExecutor.AbortPolicy());
    private volatile boolean closed;

    HomeUpdaterClient(Activity activity,BiConsumer<String,JSONObject> emit,BooleanSupplier foreground) {
        this.activity=activity;this.emit=emit;this.foreground=foreground;
    }
    void close() {closed=true;cancelPreparation();worker.shutdownNow();}
    private void cancelPreparation() {
        long retired=generation.getAndIncrement();
        synchronized(stagingLock) {
            if(stagedSession>=0&&stagedGeneration<=retired)
                HomeUpdateInstallActivity.cancelPreparation(activity,stagedSession);
        }
    }
    private boolean cancelled(long requestedGeneration) {return closed||generation.get()!=requestedGeneration;}
    private void live(long requestedGeneration) throws IOException {
        if(cancelled(requestedGeneration))throw new IOException("cancelled");
    }

    void command(String payload) {
        if(closed||payload.length()>16384)return;
        long id=0;String operation="";
        try {
            JSONObject request=new JSONObject(payload);
            Object rawId=request.get("id");
            if(!(rawId instanceof Integer||rawId instanceof Long)||(id=((Number)rawId).longValue())<=0)return;
            operation=request.getString("operation");
            final long requestId=id;final String op=operation;
            // Closing the native updater cancels preparation immediately, including
            // work queued behind a hash. Never queue cancellation behind that work.
            if(op.equals("update_cancel")) {
                cancelPreparation();reply(id,op,"cancelled","Update preparation cancelled. Any Android confirmation already open remains controlled by Android.",null);return;
            }
            final long requestedGeneration=generation.get();
            if(!op.equals("update_info")&&!op.equals("update_status")&&!op.equals("update_allow_install")&&!op.equals("update_install")) {
                reply(id,operation,"error","unknown_operation",null);return;
            }
            worker.execute(() -> {
                if(closed)return;
                try {
                    if(op.equals("update_info"))reply(requestId,op,"ok",Build.VERSION.SDK_INT<33?unsupportedReason():"",info());
                    else if(op.equals("update_status")) {
                        HomeUpdateInstallActivity.reconcile(activity);
                        SharedPreferences state=HomeUpdateInstallActivity.state(activity);
                        reply(requestId,op,state.getString("status","idle"),state.getString("message",""),info());
                    } else {
                        live(requestedGeneration);
                        PackageInfo installed=installed();
                        if(!standalone(installed)) {
                            reply(requestId,op,"unsupported",unsupportedReason(),null);return;
                        }
                        if(!foreground.getAsBoolean()) {reply(requestId,op,"error","Return to the updater before installing.",null);return;}
                        if(op.equals("update_allow_install"))allowInstall(requestId,op,requestedGeneration);
                        else install(requestId,op,request,installed,requestedGeneration);
                    }
                } catch(IOException failure) {reply(requestId,op,"cancelled".equals(failure.getMessage())?"cancelled":"error",failure.getMessage(),null);}
                  catch(Exception failure) {reply(requestId,op,"error","Android could not prepare the update. Retry after checking storage and installation settings.",null);}
            });
        } catch(java.util.concurrent.RejectedExecutionException busy) {reply(id,operation,"error","Updater is busy. Try again shortly.",null);}
          catch(Exception invalid) {if(id>0)reply(id,operation,"error","invalid_request",null);}
    }

    private PackageInfo installed() throws Exception {
        return activity.getPackageManager().getPackageInfo(activity.getPackageName(),
            Build.VERSION.SDK_INT>=28?PackageManager.GET_SIGNING_CERTIFICATES:PackageManager.GET_SIGNATURES);
    }
    private static HomeUpdatePolicy.Package metadata(PackageInfo info) throws Exception {
        if(info==null||info.applicationInfo==null)return null;
        Signature[] certificates=Build.VERSION.SDK_INT>=28
            ?(info.signingInfo==null?null:info.signingInfo.getApkContentsSigners()):info.signatures;
        String[] digests=new String[certificates==null?0:certificates.length];
        for(int i=0;i<digests.length;i++)digests[i]=HomeUpdatePolicy.digest(certificates[i].toByteArray());
        return new HomeUpdatePolicy.Package(info.packageName,versionCode(info),info.applicationInfo.minSdkVersion,digests);
    }
    private static long versionCode(PackageInfo info) {return Build.VERSION.SDK_INT>=28?info.getLongVersionCode():info.versionCode;}
    private static String unsupportedReason() {
        return Build.VERSION.SDK_INT<33?"Standalone OctoSense updates require Android 13 (API 33) or later."
            :"This installation uses a different update identity. Use the ROM or distributor updater.";
    }
    private static boolean standalone(PackageInfo info) throws Exception {
        HomeUpdatePolicy.Package value=metadata(info);
        return Build.VERSION.SDK_INT>=33&&value!=null&&HomeUpdatePolicy.standalone(value,
            (info.applicationInfo.flags&(ApplicationInfo.FLAG_SYSTEM|ApplicationInfo.FLAG_UPDATED_SYSTEM_APP))!=0);
    }
    private File cacheDir() throws IOException {
        File root=new File(activity.getCacheDir(),"octosense-updates");
        if(java.nio.file.Files.isSymbolicLink(root.toPath())||(!root.isDirectory()&&!root.mkdir()))throw new IOException("invalid_cache");
        try {android.system.Os.chmod(root.getPath(),0700);}
        catch(android.system.ErrnoException failure) {throw new IOException("private_cache_unavailable",failure);}
        return root.getCanonicalFile();
    }
    private JSONObject info() throws Exception {
        PackageInfo installed=installed();HomeUpdatePolicy.Package value=metadata(installed);
        if(value==null)throw new IOException("package_unavailable");
        return new JSONObject().put("cache_dir",cacheDir().getPath()).put("package",installed.packageName)
            .put("version_name",installed.versionName==null?"":installed.versionName).put("version_code",versionCode(installed))
            .put("sdk",Build.VERSION.SDK_INT).put("signature",new JSONArray(value.signers))
            .put("standalone",standalone(installed)).put("can_install",activity.getPackageManager().canRequestPackageInstalls());
    }
    private void allowInstall(long id,String op,long requestedGeneration) {
        main.post(() -> {
            if(cancelled(requestedGeneration)||!foreground.getAsBoolean()){reply(id,op,"cancelled","Return to the updater before changing installation settings.",null);return;}
            try {
                activity.startActivity(new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,Uri.parse("package:"+activity.getPackageName())));
                reply(id,op,"settings_opened","Allow updates from OctoSense, then return here and choose Install again.",null);
            } catch(Exception unavailable) {reply(id,op,"error","Android installation settings are unavailable.",null);}
        });
    }
    private void install(long id,String op,JSONObject request,PackageInfo installed,long requestedGeneration) throws Exception {
        if(!preparing.compareAndSet(false,true)) {reply(id,op,"error","An update is already being prepared.",null);return;}
        int sessionId=-1;boolean handedOff=false;
        PackageInstaller installer=activity.getPackageManager().getPackageInstaller();
        try {
            live(requestedGeneration);
            HomeUpdateInstallActivity.reconcile(activity);
            String state=HomeUpdateInstallActivity.state(activity).getString("status","idle");
            if(state.equals("preparing")||state.equals("awaiting_confirmation"))throw new IOException("An installation is already pending. Finish or cancel Android's confirmation first.");
            if(!activity.getPackageManager().canRequestPackageInstalls()) {reply(id,op,"permission_required","Allow OctoSense to install its update in Android Settings, then choose Install again.",null);return;}
            File file=HomeUpdatePolicy.checkedFile(cacheDir(),request.getString("path"));
            String sha=request.getString("sha256");
            long size=HomeUpdatePolicy.copyVerified(file,null,sha,() -> cancelled(requestedGeneration));
            PackageInfo candidate=activity.getPackageManager().getPackageArchiveInfo(file.getPath(),PackageManager.GET_SIGNING_CERTIFICATES);
            HomeUpdatePolicy.validatePackage(metadata(installed),metadata(candidate),Build.VERSION.SDK_INT);
            live(requestedGeneration);
            if(!foreground.getAsBoolean())throw new IOException("Update cancelled because the updater left the foreground.");
            PackageInstaller.SessionParams params=new PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL);
            params.setAppPackageName(installed.packageName);params.setSize(size);
            params.setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_REQUIRED);
            sessionId=installer.createSession(params);
            String token=UUID.randomUUID().toString();
            // Persist before the large copy so process death leaves a recoverable,
            // expiring session rather than an untracked installer allocation.
            synchronized(stagingLock) {
                live(requestedGeneration);
                stagedSession=sessionId;stagedGeneration=requestedGeneration;
                HomeUpdateInstallActivity.prepare(activity,sessionId,token,versionCode(candidate));
            }
            try(PackageInstaller.Session session=installer.openSession(sessionId);
                OutputStream out=session.openWrite("base.apk",0,size)) {
                HomeUpdatePolicy.copyVerified(HomeUpdatePolicy.checkedFile(cacheDir(),request.getString("path")),out,sha,() -> cancelled(requestedGeneration));
                session.fsync(out);
            }
            live(requestedGeneration);
            if(!foreground.getAsBoolean())throw new IOException("Update cancelled because the updater left the foreground.");
            final int preparedSession=sessionId;
            main.post(() -> {
                if(cancelled(requestedGeneration)||!foreground.getAsBoolean()) {
                    HomeUpdateInstallActivity.cancel(activity,preparedSession,"Update cancelled before Android confirmation.");
                    reply(id,op,"cancelled","Update cancelled before Android confirmation.",null);return;
                }
                try {
                    HomeUpdateInstallActivity.start(activity,token);
                    reply(id,op,"awaiting_confirmation","Review the update in Android's installer. Installation requires your confirmation.",null);
                } catch(Exception unavailable) {
                    HomeUpdateInstallActivity.cancel(activity,preparedSession,"Android's installer could not open.");
                    reply(id,op,"error","Android's installer could not open.",null);
                }
            });
            handedOff=true;
        } finally {
            if(sessionId>=0&&!handedOff)HomeUpdateInstallActivity.cancel(activity,sessionId,"Update preparation did not complete. Choose Install to retry.");
            preparing.set(false);
        }
    }
    private void reply(long id,String operation,String status,String message,JSONObject extra) {
        if(closed)return;
        try {
            JSONObject value=extra==null?new JSONObject():extra;
            value.put("id",id).put("operation",operation).put("status",status).put("message",message);
            emit.accept("home_updater.result",value);
        } catch(Exception ignored) {}
    }
}
