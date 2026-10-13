package dev.makepad.octosense;

import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageInstaller;

/** Runs the actual callback Activity against a recording fake Android boundary. */
public final class HomeUpdateInstallTest {
    static final int SESSION=73;
    static final String TOKEN="synthetic-one-use-token";
    static final String TOKEN_KEY="octosense.update.token";
    static final String START="dev.makepad.octosense.update.START",RESULT="dev.makepad.octosense.update.RESULT";
    static final Context CONTEXT=new Context();
    static void check(boolean condition,String detail){if(!condition)throw new AssertionError(detail);}
    static String status(){return HomeUpdateInstallActivity.state(CONTEXT).getString("status","idle");}
    static HomeUpdateInstallActivity launch(Intent intent){HomeUpdateInstallActivity activity=new HomeUpdateInstallActivity();activity.setIntent(intent);activity.onCreate(null);return activity;}
    static Intent result(int status){return new Intent().setAction(RESULT).putExtra(TOKEN_KEY,TOKEN).putExtra(PackageInstaller.EXTRA_SESSION_ID,SESSION).putExtra(PackageInstaller.EXTRA_STATUS,status);}
    static void begin() throws Exception {
        Context.PM.installer.sessions.add(SESSION);
        HomeUpdateInstallActivity.prepare(CONTEXT,SESSION,TOKEN,11);
    }
    static void commit() throws Exception {begin();launch(new Intent().setAction(START).putExtra(TOKEN_KEY,TOKEN));}
    public static void main(String[] args) throws Exception {
        switch(args[0]) {
        case "token": {
            begin();HomeUpdateInstallActivity bad=launch(new Intent().setAction(START).putExtra(TOKEN_KEY,"wrong"));
            check(bad.finished,"forged callback remained open");check(Context.PM.installer.commits==0,"forged callback committed");
            check(status().equals("preparing"),"forged callback changed state");break;
        }
        case "commit": {
            commit();check(Context.PM.installer.commits==1,"session was not committed exactly once");
            check(PendingIntent.lastIntent.target==HomeUpdateInstallActivity.class,"implicit callback target");
            check((PendingIntent.lastFlags&PendingIntent.FLAG_MUTABLE)!=0,"immutable callback rejected by Android35");
            check(android.app.ActivityOptions.mode==1,"Android35 background callback opt-in missing");
            launch(new Intent().setAction(START).putExtra(TOKEN_KEY,TOKEN));
            check(Context.PM.installer.commits==1,"duplicate start committed twice");break;
        }
        case "wrong_session": {
            commit();HomeUpdateInstallActivity bad=launch(result(PackageInstaller.STATUS_PENDING_USER_ACTION)
                .putExtra(PackageInstaller.EXTRA_SESSION_ID,SESSION+1).putExtra(Intent.EXTRA_INTENT,new Intent()));
            check(bad.finished&&bad.confirmation==null,"other session opened confirmation");break;
        }
        case "confirm": {
            commit();Intent platform=new Intent();HomeUpdateInstallActivity callback=launch(result(PackageInstaller.STATUS_PENDING_USER_ACTION).putExtra(Intent.EXTRA_INTENT,platform));
            check(callback.confirmation==platform,"system confirmation not opened");
            // Some OS confirmation activities return RESULT_CANCELED after handing the
            // accepted session back to PackageInstaller; only its callback is final.
            callback.onActivityResult(callback.request,android.app.Activity.RESULT_CANCELED,null);
            check(Context.PM.installer.abandoned==0,"activity return cancelled a possibly approved installation");
            check(status().equals("awaiting_confirmation"),"activity return invented a result");break;
        }
        case "cancel": {
            commit();launch(result(PackageInstaller.STATUS_FAILURE_ABORTED));
            check(status().equals("cancelled"),"system cancellation not retained");break;
        }
        case "success": {
            commit();launch(result(PackageInstaller.STATUS_SUCCESS));check(status().equals("installed"),"system success not retained");
            launch(result(PackageInstaller.STATUS_PENDING_USER_ACTION).putExtra(Intent.EXTRA_INTENT,new Intent()));
            check(status().equals("installed"),"late pending callback rewound terminal result");break;
        }
        case "failure": {
            commit();launch(result(PackageInstaller.STATUS_FAILURE));check(status().equals("error"),"system failure not retained");break;
        }
        case "recovery": {
            commit();Context.PM.info.version=11;HomeUpdateInstallActivity.reconcile(CONTEXT);
            check(status().equals("installed"),"successful self-replacement not recognized after process restart");break;
        }
        case "missing_session": {
            commit();Context.PM.installer.sessions.clear();HomeUpdateInstallActivity.reconcile(CONTEXT);
            check(status().equals("cancelled"),"missing session left installer waiting forever");break;
        }
        case "expired": {
            begin();HomeUpdateInstallActivity.state(CONTEXT).edit().putLong("started_ms",0).commit();HomeUpdateInstallActivity.reconcile(CONTEXT);
            check(Context.PM.installer.abandoned==1&&status().equals("cancelled"),"stale uncommitted session not cleaned up");break;
        }
        case "cancel_preparation": {
            begin();HomeUpdateInstallActivity.cancelPreparation(CONTEXT,SESSION);
            check(status().equals("cancelled")&&Context.PM.installer.abandoned==1,"native closure did not cancel preparation");
            launch(new Intent().setAction(START).putExtra(TOKEN_KEY,TOKEN));
            check(Context.PM.installer.commits==0,"cancelled preparation committed later");break;
        }
        case "preserve_committed": {
            commit();HomeUpdateInstallActivity.cancelPreparation(CONTEXT,SESSION);
            check(status().equals("awaiting_confirmation")&&Context.PM.installer.abandoned==0,"native closure abandoned an OS-owned confirmation");break;
        }
        case "cancel_other_session": {
            begin();HomeUpdateInstallActivity.cancelPreparation(CONTEXT,SESSION+1);
            check(status().equals("preparing")&&Context.PM.installer.abandoned==0,"stale closure cancelled a different preparation");break;
        }
        case "stale_cleanup": {
            begin();HomeUpdateInstallActivity.cancel(CONTEXT,SESSION-1,"old session cleanup");
            check(status().equals("preparing"),"old main-thread callback overwrote newer session");break;
        }
        case "missing_confirmation": {
            commit();launch(result(PackageInstaller.STATUS_PENDING_USER_ACTION));
            check(Context.PM.installer.abandoned==1&&status().equals("error"),"missing system confirmation did not fail closed");break;
        }
        default:throw new AssertionError("unknown case");
        }
    }
}
