package dev.makepad.octosense;

import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

/** The AI providers import sheet's "Choose image": the system picker for one
 * image, whose bytes land in a private cache file that Rust reads (and
 * deletes) to decode a provider QR from.
 *
 * A helper Activity, not Home's own: Home is singleInstance, and a result
 * requested from a singleInstance activity can come back cancelled at once
 * (the picker has to open in another task). This one is a standard,
 * invisible activity in a task of its own; it asks for the result, copies
 * the chosen document off the main thread and reports exactly once.
 */
public final class QrImagePickActivity extends Activity {
    /** Answered once per pick: status "ok" (path), "cancelled" or "error" (reason). */
    public interface Listener { void onPicked(long id,String status,String detail); }

    static final String EXTRA_ID="dev.makepad.octosense.qr_pick_id";
    private static final int REQUEST=0x0A17;
    private static final long MAX_BYTES=16L*1024*1024;
    private static volatile Listener listener;
    private long id;
    private boolean answered;

    static void setListener(Listener l) { listener=l; }

    /** Opens the picker; the answer arrives on the listener. */
    static void start(Context context,long id) {
        Intent intent=new Intent(context,QrImagePickActivity.class);
        intent.putExtra(EXTRA_ID,id);
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_NO_ANIMATION);
        context.startActivity(intent);
    }

    /** Where a picked image waits for Rust; one at a time. */
    static File target(Context context) { return new File(context.getCacheDir(),"qr-import-image"); }

    @Override protected void onCreate(Bundle state) {
        super.onCreate(state);
        id=getIntent().getLongExtra(EXTRA_ID,0);
        if(state!=null) { answered=state.getBoolean("answered"); return; } // the picker is already up
        Intent pick=new Intent(Intent.ACTION_OPEN_DOCUMENT);
        pick.addCategory(Intent.CATEGORY_OPENABLE);
        pick.setType("image/*");
        pick.putExtra(Intent.EXTRA_MIME_TYPES,new String[]{"image/png","image/jpeg"});
        try { startActivityForResult(pick,REQUEST); }
        catch(Exception e) { answer("error","picker_unavailable"); finish(); }
    }

    @Override protected void onSaveInstanceState(Bundle out) {
        super.onSaveInstanceState(out);
        out.putBoolean("answered",answered);
    }

    @Override protected void onActivityResult(int request,int result,Intent data) {
        if(request!=REQUEST) { super.onActivityResult(request,result,data); return; }
        final Uri uri=result==RESULT_OK&&data!=null ? data.getData() : null;
        if(uri==null) { answer("cancelled",""); finish(); return; }
        answered=true; // the copy below answers
        final Context app=getApplicationContext();
        final long pickId=id;
        new Thread(() -> copy(app,uri,pickId),"OctoSenseQrImage").start();
        finish();
    }

    @Override protected void onDestroy() {
        // Leaving without a pick (the activity was dismissed or recreated
        // away) still answers, so the sheet is never left waiting.
        if(isFinishing()&&!answered) answer("cancelled","");
        super.onDestroy();
    }

    private void answer(String status,String detail) {
        if(answered) return;
        answered=true;
        report(id,status,detail);
    }

    private static void report(long id,String status,String detail) {
        Listener l=listener;
        if(l!=null) l.onPicked(id,status,detail);
    }

    private static void copy(Context context,Uri uri,long id) {
        File out=target(context);
        try(InputStream in=context.getContentResolver().openInputStream(uri);
            OutputStream sink=new FileOutputStream(out)) {
            if(in==null) throw new java.io.IOException("unreadable");
            byte[] buffer=new byte[64*1024];
            long total=0;
            int n;
            while((n=in.read(buffer))>0) {
                total+=n;
                if(total>MAX_BYTES) { sink.close(); out.delete(); report(id,"error","too_large"); return; }
                sink.write(buffer,0,n);
            }
        } catch(Exception e) {
            out.delete();
            report(id,"error","unreadable");
            return;
        }
        report(id,"ok",out.getAbsolutePath());
    }
}
