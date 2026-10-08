package dev.makepad.octosense;

import android.graphics.Rect;
import android.os.Build;
import android.view.View;
import dev.makepad.android.MakepadActivity;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import org.json.JSONArray;
import org.json.JSONObject;

/** Home's pager owns side swipes; hosted apps, cards and the IME own Back.
 * Uses the public window API. Android exempts HOME activities from the
 * ordinary 200dp exclusion limit, so paging works at every body height.
 * No global gesture setting or privileged window flag is changed.
 */
final class HomeGestureEdges {
    private final MakepadActivity activity;
    private final View root;
    private List<Rect> requested=Collections.emptyList(),applied=Collections.emptyList();
    private boolean resumed,focused,covered,closed;
    private final View.OnLayoutChangeListener resize=(v,l,t,r,b,ol,ot,or,ob) -> {
        if(l!=ol||t!=ot||r!=or||b!=ob) {requested=Collections.emptyList();apply();}
    };

    HomeGestureEdges(MakepadActivity activity) {
        this.activity=activity;
        root=activity.getApplicationOverlay();
        root.addOnLayoutChangeListener(resize);
    }
    void resume() {resumed=true;focused=activity.hasWindowFocus();apply();}
    void pause() {resumed=false;apply();}
    void focus(boolean value) {focused=value;apply();}
    void cover(boolean value) {covered=value;apply();}
    void close() {closed=true;apply();root.removeOnLayoutChangeListener(resize);}

    /** Same window-pixel geometry as home.layout; independent of its ready
     * flag, which becomes false during a page drag or settling animation. */
    void layout(String payload) {
        if(closed||Build.VERSION.SDK_INT<29) return;
        List<Rect> next=new ArrayList<>();
        try {
            JSONObject value=new JSONObject(payload);
            if(Math.abs(value.getDouble("pixel_width")-root.getWidth())>1
                    ||Math.abs(value.getDouble("pixel_height")-root.getHeight())>1)
                throw new IllegalArgumentException("Stale pager viewport");
            JSONArray edges=value.getJSONArray("pager_edges");
            if(edges.length()!=0&&edges.length()!=2) throw new IllegalArgumentException("Pager edges");
            for(int i=0;i<edges.length();i++) {
                JSONArray edge=edges.getJSONArray(i);
                if(edge.length()!=4) throw new IllegalArgumentException("Pager bounds");
                Rect rect=new Rect(edge.getInt(0),edge.getInt(1),edge.getInt(2),edge.getInt(3));
                if(rect.isEmpty()||rect.left<0||rect.top<0||rect.right>root.getWidth()||rect.bottom>root.getHeight())
                    throw new IllegalArgumentException("Pager bounds outside window");
                next.add(rect);
            }
        } catch(Exception invalid) {next.clear();}
        requested=next;apply();
    }
    @android.annotation.TargetApi(29)
    private void apply() {
        if(Build.VERSION.SDK_INT<29) return;
        List<Rect> next=resumed&&focused&&!covered&&!closed?requested:Collections.emptyList();
        if(!next.equals(applied)) {
            activity.getWindow().setSystemGestureExclusionRects(next);
            applied=new ArrayList<>(next);
        }
    }
}
