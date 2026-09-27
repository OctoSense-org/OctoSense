package dev.makepad.octosense.contracts;

import android.content.ComponentName;
import android.os.Bundle;

/** Bounded transition lifecycle packet, ordered with extension-state callbacks. */
public final class HomeTransitionEvent {
    public static final int STARTED=0,PROGRESS=1,FINISHED=2,CANCELLED=3;
    public final long id,layoutRevision,eventRevision,userSerial;
    public final String session,epoch;
    public final ComponentName component;
    public final int phase,displayId,rotation;
    public final float progress;

    private HomeTransitionEvent(long id,long revision,Bundle value) {
        if(id<=0||revision< -1||value==null||value.getInt("version",-1)!=1)
            throw new IllegalArgumentException("Invalid transition identity or version");
        this.id=id;layoutRevision=revision;
        session=value.getString("session");epoch=value.getString("epoch");
        Protocol.requireSession(session);Protocol.requireSession(epoch);
        eventRevision=value.getLong("event_revision",-1);userSerial=value.getLong("user",-1);
        phase=value.getInt("phase",-1);displayId=value.getInt("display_id",-1);rotation=value.getInt("rotation",-1);
        progress=value.getFloat("progress",Float.NaN);Protocol.requireUnitValue(progress);
        String name=value.getString("component");
        component=name==null||name.length()>512?null:ComponentName.unflattenFromString(name);
        if(eventRevision<=0||userSerial<0||phase<STARTED||phase>CANCELLED||displayId<0||rotation<0||rotation>3
                ||component==null||component.getPackageName().isEmpty()||component.getClassName().isEmpty())
            throw new IllegalArgumentException("Invalid transition target or ordering");
        if((phase==STARTED&&progress!=0)||(phase==FINISHED&&progress!=1))
            throw new IllegalArgumentException("Invalid transition boundary");
    }
    public static HomeTransitionEvent decode(long id,long revision,Bundle value) {
        return new HomeTransitionEvent(id,revision,value);
    }
}
