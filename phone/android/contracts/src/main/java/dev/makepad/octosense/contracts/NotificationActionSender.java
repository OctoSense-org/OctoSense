package dev.makepad.octosense.contracts;

import android.app.PendingIntent;
import android.app.RemoteInput;
import android.content.Context;
import android.content.Intent;
import android.os.Build;
import android.os.Bundle;

/** Native-only action dispatch. Callers must validate the live notification's
 * opaque handle before entering here. No PendingIntent or RemoteInput leaves
 * the native process in the Home presentation model.
 */
public final class NotificationActionSender {
    private NotificationActionSender() {}
    public static boolean acceptsReply(RemoteInput[] inputs) {
        if(inputs!=null) for(RemoteInput input:inputs) if(input.getAllowFreeFormInput()) return true;
        return false;
    }
    public static void send(Context context,PendingIntent target,RemoteInput[] inputs,String reply)
            throws PendingIntent.CanceledException {
        boolean freeform=acceptsReply(inputs);
        if(freeform&&reply==null) throw new IllegalArgumentException("Reply text required");
        Intent fill=new Intent();
        if(reply!=null) {
            if(!freeform||reply.length()>2000||reply.trim().isEmpty()) throw new IllegalArgumentException("Invalid reply");
            if(Build.VERSION.SDK_INT>=31&&target.isImmutable()) throw new IllegalArgumentException("Reply action is immutable");
            Bundle results=new Bundle();
            for(RemoteInput input:inputs) if(input.getAllowFreeFormInput()) results.putCharSequence(input.getResultKey(),reply);
            RemoteInput.addResultsToIntent(inputs,fill,results);
            if(Build.VERSION.SDK_INT>=28) RemoteInput.setResultsSource(fill,RemoteInput.SOURCE_FREE_FORM_INPUT);
        }
        target.send(context,0,fill);
    }
}
