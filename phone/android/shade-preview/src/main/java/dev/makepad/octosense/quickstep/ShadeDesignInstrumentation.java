package dev.makepad.octosense.quickstep;

import android.app.Instrumentation;
import android.content.Intent;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.os.Bundle;
import android.view.View;
import android.view.ViewGroup;
import android.widget.TextView;
import java.io.File;
import java.io.FileOutputStream;

/** Captures only the fixture app's own view. Never captures the display or other windows. */
public final class ShadeDesignInstrumentation extends Instrumentation {
    private Bundle arguments;
    @Override public void onCreate(Bundle args) { super.onCreate(args); arguments = args; start(); }
    @Override public void onStart() {
        Bundle result = new Bundle();
        android.app.Activity activity = null;
        try {
            String scenario = arguments.getString("scenario", "controls");
            Intent intent = new Intent(getTargetContext(), scenario.equals("onboarding") ? GlobalShadeSettingsActivity.class : ShadeDesignActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            intent.putExtra("scenario", scenario);
            intent.putExtra("font_scale", Float.parseFloat(arguments.getString("font_scale", "1")));
            intent.putExtra("light", arguments.getString("light", "false").equals("true"));
            intent.putExtra("rtl", arguments.getString("rtl", "false").equals("true"));
            activity = startActivitySync(intent);
            waitForIdleSync(); Thread.sleep(600);
            final View panel = ((ViewGroup)activity.findViewById(android.R.id.content)).getChildAt(0);
            String action = arguments.getString("action", "");
            if (!action.isEmpty()) {
                runOnMainSync(() -> { View control = find(panel, action); if (control == null) throw new IllegalStateException("Missing fixture action " + action); control.performClick(); });
                waitForIdleSync(); Thread.sleep(700);
            }
            String name = arguments.getString("name", scenario);
            if (!name.matches("[a-z0-9_-]+")) throw new IllegalArgumentException("Invalid capture name");
            File destination = new File(getTargetContext().getExternalFilesDir(null), "design/" + name + ".png");
            destination.getParentFile().mkdirs();
            runOnMainSync(() -> {
                int[] small = {0}; audit(panel, small);
                if (small[0] != 0) throw new IllegalStateException("Visible targets below 48dp: " + small[0]);
                result.putInt("small_touch_targets", small[0]);
                if (scenario.equals("controls") && action.isEmpty() && arguments.getString("font_scale","1").equals("1")) {
                    for(String label:new String[]{"Wi-Fi","Bluetooth","Flashlight","Rotation lock","Do Not Disturb","Battery saver","Brightness","Media volume"}) {
                        View control=find(panel,label); android.graphics.Rect area=new android.graphics.Rect();
                        if(control==null || !control.getGlobalVisibleRect(area) || area.height()<control.getHeight())
                            throw new IllegalStateException("Primary control below initial fold: "+label);
                    }
                    result.putBoolean("all_primary_controls_visible",true);
                }
                if(action.equals("Reply")) {
                    View send=find(panel,"Send");
                    android.graphics.Rect area=new android.graphics.Rect();
                    if(send==null || !send.getGlobalVisibleRect(area) || area.height()<send.getHeight())
                        throw new IllegalStateException("Reply action clipped by keyboard");
                    result.putBoolean("reply_actions_visible",true);
                }
                Bitmap bitmap = Bitmap.createBitmap(panel.getWidth(), panel.getHeight(), Bitmap.Config.ARGB_8888);
                panel.draw(new Canvas(bitmap));
                if(scenario.equals("controls") && action.isEmpty() && arguments.getString("font_scale","1").equals("1")) {
                    verifyTrack(panel, bitmap, "Brightness", .2f, .8f);
                    verifyTrack(panel, bitmap, "Media volume", .1f, .9f);
                    result.putBoolean("slider_fill_matches_direction",true);
                }
                try (FileOutputStream output = new FileOutputStream(destination)) { bitmap.compress(Bitmap.CompressFormat.PNG, 100, output); }
                catch (Exception error) { throw new RuntimeException(error); }
                bitmap.recycle();
            });
            result.putString("result", "pass"); result.putString("capture", destination.getAbsolutePath());
            result.putInt("width", panel.getWidth()); result.putInt("height", panel.getHeight());
            result.putString("source", "fixture-owned View.draw(Canvas), exact production panel source");
            finish(0, result);
        } catch (Exception error) { result.putString("failure", error.toString()); finish(1, result); }
        finally { if (activity != null) { final android.app.Activity current = activity; runOnMainSync(current::finish); } }
    }
    private android.widget.SeekBar seek(View view,String label) {
        if(view instanceof android.widget.SeekBar && label.contentEquals(view.getContentDescription())) return (android.widget.SeekBar)view;
        if(view instanceof ViewGroup) for(int i=0;i<((ViewGroup)view).getChildCount();i++) {
            android.widget.SeekBar result=seek(((ViewGroup)view).getChildAt(i),label); if(result!=null) return result;
        }
        return null;
    }
    private void verifyTrack(View panel,Bitmap bitmap,String label,float left,float right) {
        android.widget.SeekBar bar=seek(panel,label); if(bar==null) throw new IllegalStateException("Missing slider "+label);
        int[] root=new int[2], location=new int[2]; panel.getLocationOnScreen(root); bar.getLocationOnScreen(location);
        int start=location[0]-root[0]+bar.getPaddingLeft(), width=bar.getWidth()-bar.getPaddingLeft()-bar.getPaddingRight();
        int y=location[1]-root[1]+bar.getHeight()/2;
        int a=bitmap.getPixel(start+Math.round(width*left),y), b=bitmap.getPixel(start+Math.round(width*right),y);
        GlobalShadeStyle ui=new GlobalShadeStyle(bar.getContext()); boolean rtl=bar.getLayoutDirection()==View.LAYOUT_DIRECTION_RTL;
        if(a!=(rtl?ui.elevated:ui.accent) || b!=(rtl?ui.accent:ui.elevated))
            throw new IllegalStateException("Slider fill direction mismatch: "+label+" rtl="+rtl);
    }
    private void audit(View view,int[] small) {
        android.graphics.Rect visible = new android.graphics.Rect();
        if(view.getVisibility()!=View.VISIBLE || !view.getGlobalVisibleRect(visible)) return;
        if(view.isClickable() || view instanceof android.widget.SeekBar) {
            float min=48*view.getResources().getDisplayMetrics().density;
            if(view.getWidth()+1<min || view.getHeight()+1<min) small[0]++;
        }
        if(view instanceof ViewGroup) for(int i=0;i<((ViewGroup)view).getChildCount();i++) audit(((ViewGroup)view).getChildAt(i),small);
    }
    private View find(View view, String label) {
        if (view instanceof TextView && label.contentEquals(((TextView) view).getText())) return view;
        if (label.contentEquals(view.getContentDescription() == null ? "" : view.getContentDescription())) return view;
        if (view instanceof ViewGroup) for (int i = 0; i < ((ViewGroup) view).getChildCount(); i++) {
            View found = find(((ViewGroup) view).getChildAt(i), label); if (found != null) return found;
        }
        return null;
    }
}
