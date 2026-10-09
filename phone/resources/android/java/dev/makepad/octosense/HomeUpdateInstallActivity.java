package dev.makepad.octosense;

import android.app.Activity;
import android.app.ActivityOptions;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageInstaller;
import android.net.Uri;
import android.os.Bundle;

/** A non-exported callback for one reviewed PackageInstaller session. A persisted,
 * unpredictable token binds every callback to that session even after process death.
 * The system installer, not this Activity or the Rust UI, approves the APK update. */
public final class HomeUpdateInstallActivity extends Activity {
    private static final String TOKEN="octosense.update.token";
    private static final String START="dev.makepad.octosense.update.START";
    private static final String RESULT="dev.makepad.octosense.update.RESULT";
    private static final int CONFIRM=0x5531;
    private boolean confirmationOpen;

    static SharedPreferences state(Context context) {return context.getSharedPreferences("home-update-session",Context.MODE_PRIVATE);}
    static synchronized void prepare(Context context,int session,String token,long version) throws java.io.IOException {
        if(!state(context).edit().clear().putInt("session",session).putString("token",token)
                .putLong("version",version).putLong("started_ms",System.currentTimeMillis()).putString("status","preparing").putString("message","Preparing Android confirmation.").commit())
            throw new java.io.IOException("Update state could not be saved.");
    }
    static void start(Context context,String token) {
        context.startActivity(new Intent(context,HomeUpdateInstallActivity.class).setAction(START)
            .putExtra(TOKEN,token).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
    }
    static synchronized void record(Context context,String status,String message) {
        state(context).edit().putString("status",status).putString("message",message).apply();
    }
    static synchronized void cancel(Context context,int session,String message) {
        try{context.getPackageManager().getPackageInstaller().abandonSession(session);}catch(Exception ignored){}
        // A queued main-thread callback may belong to an earlier preparation.
        // Its cleanup must not overwrite the status of a newer session.
        if(state(context).getInt("session",-1)==session)record(context,"cancelled",message);
    }
    /** Native window closure only cancels preparation. Once commit begins the
     * Android-owned confirmation and result must survive renderer shutdown. */
    static synchronized void cancelPreparation(Context context,int session) {
        SharedPreferences prefs=state(context);
        if(prefs.getInt("session",-1)==session&&prefs.getString("status","").equals("preparing"))
            cancel(context,session,"Update preparation cancelled.");
    }
    static synchronized void reconcile(Context context) {
        SharedPreferences prefs=state(context);String status=prefs.getString("status","idle");
        if(!status.equals("preparing")&&!status.equals("awaiting_confirmation"))return;
        int session=prefs.getInt("session",-1);
        try {
            if(context.getPackageManager().getPackageInfo(context.getPackageName(),0).getLongVersionCode()>=prefs.getLong("version",Long.MAX_VALUE)) {
                record(context,"installed","OctoSense is up to date.");return;
            }
            if(session<0||context.getPackageManager().getPackageInstaller().getSessionInfo(session)==null)
                record(context,"cancelled","The previous update did not complete. Choose Install to retry.");
            else {
                long elapsed=System.currentTimeMillis()-prefs.getLong("started_ms",0);
                long deadline=status.equals("preparing")?120000L:86400000L;
                if(elapsed<0||elapsed>deadline)cancel(context,session,"The previous installation expired. Choose Install to retry.");
            }
        } catch(Exception unavailable) {record(context,"error","Could not read the previous Android installation result.");}
    }
    @Override protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        if(android.os.Build.VERSION.SDK_INT>=31)getWindow().setHideOverlayWindows(true);
        confirmationOpen=saved!=null&&saved.getBoolean("confirmation_open",false);
        if(saved==null)handle(getIntent());
    }
    @Override protected void onNewIntent(Intent intent) {super.onNewIntent(intent);setIntent(intent);handle(intent);}
    @Override protected void onSaveInstanceState(Bundle out) {super.onSaveInstanceState(out);out.putBoolean("confirmation_open",confirmationOpen);}

    private void handle(Intent intent) {
        synchronized(HomeUpdateInstallActivity.class) {handleLocked(intent);}
    }
    private void handleLocked(Intent intent) {
        SharedPreferences prefs=state(this);
        String token=intent==null?null:intent.getStringExtra(TOKEN);
        String expected=prefs.getString("token",null);
        int sessionId=prefs.getInt("session",-1);
        if(token==null||expected==null||!expected.equals(token)||sessionId<0) {finish();return;}
        String current=prefs.getString("status","idle");
        if(!current.equals("preparing")&&!current.equals("awaiting_confirmation")){finish();return;}
        try {
            if(START.equals(intent.getAction())) {
                if(!current.equals("preparing")){finish();return;}
                Intent callback=new Intent(this,HomeUpdateInstallActivity.class).setAction(RESULT)
                    .setData(Uri.parse("octosense-update:"+token)).putExtra(TOKEN,token)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_SINGLE_TOP);
                // PackageInstaller fills in its result, so this explicit, non-exported
                // target needs a mutable PendingIntent (immutable is rejected on API35).
                PendingIntent pending=PendingIntent.getActivity(this,sessionId,callback,
                    PendingIntent.FLAG_UPDATE_CURRENT|PendingIntent.FLAG_MUTABLE,callbackOptions());
                record(this,"awaiting_confirmation","Review and confirm the update in Android's installer.");
                try(PackageInstaller.Session session=getPackageManager().getPackageInstaller().openSession(sessionId)) {
                    session.commit(pending.getIntentSender());
                }
                return;
            }
            if(!RESULT.equals(intent.getAction())||intent.getIntExtra(PackageInstaller.EXTRA_SESSION_ID,-1)!=sessionId) {finish();return;}
            int result=intent.getIntExtra(PackageInstaller.EXTRA_STATUS,PackageInstaller.STATUS_FAILURE);
            if(result==PackageInstaller.STATUS_PENDING_USER_ACTION) {
                Intent confirmation=intent.getParcelableExtra(Intent.EXTRA_INTENT);
                if(confirmation==null)throw new IllegalStateException("missing_confirmation");
                confirmationOpen=true;startActivityForResult(confirmation,CONFIRM);
            } else {
                if(result==PackageInstaller.STATUS_SUCCESS)record(this,"installed","OctoSense was updated successfully.");
                else if(result==PackageInstaller.STATUS_FAILURE_ABORTED)record(this,"cancelled","Installation was cancelled. Your current OctoSense is unchanged.");
                else record(this,"error","Android rejected the update (status "+result+"). Your current OctoSense is unchanged.");
                finish();
            }
        } catch(Exception unavailable) {
            cancel(this,sessionId,"Android's installer could not complete the update.");
            record(this,"error","Android's installer could not complete the update.");finish();
        }
    }
    /** Android 15 requires the PendingIntent creator to opt in to launching the
     * callback. Keep API33 packaging compatibility; reflect only this API35 call.
     * The mutable token-bearing intent is handed only to PackageInstaller. */
    private static Bundle callbackOptions() throws Exception {
        if(android.os.Build.VERSION.SDK_INT<35)return null;
        ActivityOptions options=ActivityOptions.makeBasic();
        int allowed=ActivityOptions.class.getField("MODE_BACKGROUND_ACTIVITY_START_ALLOWED").getInt(null);
        ActivityOptions.class.getMethod("setPendingIntentCreatorBackgroundActivityStartMode",int.class).invoke(options,allowed);
        return options.toBundle();
    }
    @Override protected void onActivityResult(int request,int result,Intent data) {
        super.onActivityResult(request,result,data);
        if(request!=CONFIRM)return;
        confirmationOpen=false;
        // PackageInstaller's status callback is authoritative. Some confirmation
        // Activities return RESULT_CANCELED even after submitting an accepted
        // session; abandoning here could cancel an update the person approved.
        reconcile(this);
        finish();
    }
    @Override public void onBackPressed() {
        String token=getIntent()==null?null:getIntent().getStringExtra(TOKEN);
        if(!confirmationOpen&&token!=null&&token.equals(state(this).getString("token",null)))
            cancel(this,state(this).getInt("session",-1),"Installation was cancelled. Your current OctoSense is unchanged.");
        super.onBackPressed();
    }
}
