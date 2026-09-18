package dev.makepad.octosense.contracts;

import android.content.ComponentName;
import android.os.Bundle;
import java.util.ArrayList;
import java.util.HashSet;

/** Version 1 Home geometry: display pixels, not dp or activity-local pixels.
 * Insets are left/top/right/bottom distances; viewport and icon bounds are
 * left/top/right/bottom coordinates. Only actually visible app icons qualify.
 * This model carries no surfaces, Binder handles, shortcuts or guessed targets.
 */
public final class HomeLayout {
    public static final int MAX_ICONS=128;
    public final long revision,transitionId;
    public final int displayId,rotation;
    public final String epoch;
    private final Bundle packet;
    private HomeLayout(long revision,long transitionId,int displayId,int rotation,String epoch,Bundle packet) {
        this.revision=revision;this.transitionId=transitionId;this.displayId=displayId;this.rotation=rotation;this.epoch=epoch;this.packet=packet;
    }
    public Bundle bundle() {return packet.deepCopy();}
    public int iconCount() {return packet.getParcelableArrayList("icons").size();}
    /** Worker-side target selection. Returned bounds cannot mutate this snapshot. */
    public float[] boundsFor(ComponentName component,long userSerial) {
        if(component==null||userSerial<0) return null;
        String name=component.flattenToString();
        ArrayList<Bundle> icons=packet.getParcelableArrayList("icons");
        for(Bundle icon:icons) {
            if(userSerial==icon.getLong("user")&&name.equals(icon.getString("component")))
                return icon.getFloatArray("bounds").clone();
        }
        return null;
    }
    public static HomeLayout decode(long revision,Bundle source) {
        if(source==null||revision<=0||source.getInt("version",-1)!=1) throw new IllegalArgumentException("Invalid layout version or revision");
        // Zero describes ordinary/legacy layout publication. Native controllers
        // require a positive identity issued for the current transition.
        long transition=source.containsKey("transition_id")?source.getLong("transition_id",-1):0;
        if(transition<0) throw new IllegalArgumentException("Invalid transition identity");
        String epoch=source.getString("epoch");Protocol.requireSession(epoch);
        int display=source.getInt("display_id",-1),rotation=source.getInt("rotation",-1);
        if(display<0||rotation<0||rotation>3) throw new IllegalArgumentException("Invalid display or rotation");
        int[] viewport=source.getIntArray("viewport"),insets=source.getIntArray("insets");
        if(viewport==null||viewport.length!=4||insets==null||insets.length!=4) throw new IllegalArgumentException("Missing viewport or insets");
        if(viewport[0]<0||viewport[1]<0||viewport[2]<=viewport[0]||viewport[3]<=viewport[1]
                ||viewport[2]>32768||viewport[3]>32768) throw new IllegalArgumentException("Invalid viewport");
        for(int value:insets) if(value<0||value>32768) throw new IllegalArgumentException("Invalid inset");
        if((long)insets[0]+insets[2]>=viewport[2]-viewport[0]||(long)insets[1]+insets[3]>=viewport[3]-viewport[1])
            throw new IllegalArgumentException("Insets cover viewport");
        ArrayList<Bundle> icons=source.getParcelableArrayList("icons");
        if(icons==null||icons.size()>MAX_ICONS) throw new IllegalArgumentException("Invalid icon count");
        ArrayList<Bundle> copied=new ArrayList<>();HashSet<String> seen=new HashSet<>();
        for(Bundle icon:icons) {
            if(icon==null) throw new IllegalArgumentException("Missing icon");
            String name=icon.getString("component");long user=icon.getLong("user",-1);
            ComponentName component=name==null||name.length()>512?null:ComponentName.unflattenFromString(name);
            if(component==null||component.getPackageName().isEmpty()||component.getClassName().isEmpty()||user<0)
                throw new IllegalArgumentException("Invalid component or user serial");
            name=component.flattenToString();
            if(!seen.add(user+":"+name)) throw new IllegalArgumentException("Duplicate component/profile target");
            float[] rect=icon.getFloatArray("bounds");
            if(rect==null||rect.length!=4) throw new IllegalArgumentException("Missing icon bounds");
            for(float value:rect) if(!Float.isFinite(value)) throw new IllegalArgumentException("Non-finite icon bounds");
            if(rect[0]<viewport[0]||rect[1]<viewport[1]||rect[2]>viewport[2]||rect[3]>viewport[3]||rect[2]<=rect[0]||rect[3]<=rect[1])
                throw new IllegalArgumentException("Icon outside viewport");
            Bundle clean=new Bundle();clean.putString("component",name);clean.putLong("user",user);clean.putFloatArray("bounds",rect.clone());copied.add(clean);
        }
        Bundle clean=new Bundle();clean.putInt("version",1);clean.putLong("transition_id",transition);clean.putString("epoch",epoch);clean.putInt("display_id",display);clean.putInt("rotation",rotation);
        clean.putIntArray("viewport",viewport.clone());clean.putIntArray("insets",insets.clone());clean.putParcelableArrayList("icons",copied);
        return new HomeLayout(revision,transition,display,rotation,epoch,clean);
    }
}
