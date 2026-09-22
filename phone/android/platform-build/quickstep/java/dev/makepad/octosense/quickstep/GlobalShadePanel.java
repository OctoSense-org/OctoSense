package dev.makepad.octosense.quickstep;

import android.content.Context;
import android.content.res.ColorStateList;
import android.graphics.Typeface;
import android.graphics.drawable.ClipDrawable;
import android.graphics.drawable.Drawable;
import android.graphics.drawable.LayerDrawable;
import android.os.Bundle;
import android.text.InputFilter;
import android.text.TextUtils;
import android.text.format.DateUtils;
import android.view.Gravity;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.inputmethod.InputMethodManager;
import android.widget.Button;
import android.widget.EditText;
import android.widget.ImageButton;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.SeekBar;
import android.widget.TextView;
import dev.makepad.octosense.contracts.Protocol;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Objects;

/** Native OctoSense surface. Authority and commands remain in the controller. */
final class GlobalShadePanel extends LinearLayout {
    private final GlobalShadeController owner;
    private final GlobalShadeStyle ui;
    private final LinearLayout body, tabs, replyBar;
    private final ScrollView scroll;
    private final TextView title, date, feedback;
    private final Button notificationsTab, controlsTab, setup;
    private final ImageButton back;
    private final boolean wide;
    private Bundle state = Bundle.EMPTY;
    private boolean controls, sliding, settingsPage;
    private float downY;
    private String replyHandle;
    private EditText editor;
    private Button send;
    private long replyCommand = -1;
    private int renderedSignature;
    private final HashSet<String> expanded = new HashSet<>();

    GlobalShadePanel(Context context, GlobalShadeController owner, boolean controls) {
        super(context); this.owner = owner; this.controls = controls; ui = new GlobalShadeStyle(context);
        wide = getResources().getConfiguration().screenWidthDp >= 600;
        setOrientation(VERTICAL); setBackgroundColor(ui.background);
        setFocusableInTouchMode(true); setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_YES);
        setOnApplyWindowInsetsListener((v, insets) -> {
            android.graphics.Insets safe = insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
            android.graphics.Insets keyboard = insets.getInsets(WindowInsets.Type.ime());
            int side = Math.max(dp(20), (getWidth() - safe.left - safe.right - dp(680)) / 2);
            setPadding(side + safe.left, dp(10) + safe.top, side + safe.right, dp(8) + Math.max(safe.bottom, keyboard.bottom));
            return insets;
        });
        LinearLayout heading = horizontal(); heading.setPadding(0, dp(4), 0, dp(wide ? 8 : 16));
        back = ui.iconButton("back", "Back to panel", () -> { clearReply(); settingsPage = false; render(); });
        back.setVisibility(GONE); LayoutParams backParams = new LayoutParams(dp(48), dp(48)); backParams.setMarginEnd(dp(12)); heading.addView(back, backParams);
        LinearLayout names = vertical();
        date = ui.label(getResources().getConfiguration().fontScale > 1.3f ? "OCTOSENSE" : "OCTOSENSE  /  " + android.text.format.DateFormat.format("EEE, MMM d", System.currentTimeMillis()), 11, ui.muted);
        date.setLetterSpacing(.08f); names.addView(date);
        title = ui.label("", wide ? 25 : 32, ui.text); ui.medium(title); title.setAccessibilityHeading(true); title.setPadding(0, dp(wide ? 0 : 8), 0, 0); names.addView(title);
        LayoutParams nameParams = new LayoutParams(0, -2, 1); nameParams.setMarginEnd(dp(10)); heading.addView(names, nameParams);
        if (wide) {
            ImageButton settings = ui.iconButton("settings", "System setup", this::openSettings);
            LayoutParams p = new LayoutParams(dp(48),dp(48)); p.setMarginEnd(dp(8)); heading.addView(settings,p);
        }
        ImageButton close = ui.iconButton("close", "Close OctoSense panel", owner::dismiss);
        heading.addView(close, new LayoutParams(dp(48), dp(48))); addView(heading);
        tabs = horizontal(); tabs.setPadding(dp(4), dp(4), dp(4), dp(4)); tabs.setBackground(ui.shape(ui.dark ? ui.surface : ui.elevated, 20));
        notificationsTab = tab("Notifications", false); controlsTab = tab("Controls", true);
        tabs.addView(notificationsTab, new LayoutParams(0, -2, 1)); tabs.addView(controlsTab, new LayoutParams(0, -2, 1));
        if(wide) { LayoutParams p=new LayoutParams(0,-2,1.7f); p.setMarginEnd(dp(12)); heading.addView(tabs,2,p); }
        else addView(tabs);
        feedback = ui.label("", 13, ui.accent); feedback.setPadding(dp(12), dp(10), dp(12), dp(10));
        feedback.setAccessibilityLiveRegion(ACCESSIBILITY_LIVE_REGION_POLITE); feedback.setVisibility(GONE); addView(feedback);
        scroll = new ScrollView(context); scroll.setFillViewport(true); scroll.setClipToPadding(false);
        scroll.setVerticalScrollBarEnabled(false); scroll.setOverScrollMode(OVER_SCROLL_IF_CONTENT_SCROLLS);
        body = vertical(); body.setPadding(0, dp(wide ? 8 : 16), 0, dp(8)); scroll.addView(body);
        addView(scroll, new LayoutParams(-1, 0, 1));
        replyBar=horizontal(); replyBar.setPadding(0,dp(8),0,dp(4)); replyBar.setVisibility(GONE);
        addView(replyBar,new LayoutParams(-1,-2));
        setup = ui.button("System setup", this::openSettings, false);
        setup.setCompoundDrawablesRelativeWithIntrinsicBounds(ui.icon("settings", ui.muted), null, ui.icon("chevron",ui.muted), null);
        setup.setGravity(Gravity.START | Gravity.CENTER_VERTICAL);
        setup.setCompoundDrawablePadding(dp(10)); setup.setBackground(ui.touch(0x00000000, 16)); setup.setTextColor(ui.muted);
        addView(setup, new LayoutParams(-1, -2));
        names.setOnTouchListener((v, event) -> {
            if (event.getActionMasked() == MotionEvent.ACTION_DOWN) { downY = event.getRawY(); return true; }
            if (event.getActionMasked() == MotionEvent.ACTION_UP) {
                if (event.getRawY() - downY < -dp(48)) owner.dismiss(); else v.performClick();
            }
            return true;
        });
    }
    private void openSettings() { clearReply(); settingsPage = !settingsPage; scroll.scrollTo(0,0); render(); }
    @Override protected void onAttachedToWindow() {
        super.onAttachedToWindow();
        WindowInsetsController bars = getWindowInsetsController();
        int light = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS | WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS;
        if (bars != null) bars.setSystemBarsAppearance(ui.dark ? 0 : light, light);
        if (android.animation.ValueAnimator.areAnimatorsEnabled()) {
            setAlpha(0); setTranslationY(-dp(8)); animate().alpha(1).translationY(0).setDuration(160)
                    .setInterpolator(new android.view.animation.PathInterpolator(.2f,0,0,1)).start();
        }
    }
    private int dp(float value) { return ui.dp(value); }
    private LinearLayout horizontal() { LinearLayout row = new LinearLayout(getContext()); row.setOrientation(HORIZONTAL); row.setGravity(Gravity.CENTER_VERTICAL); return row; }
    private LinearLayout vertical() { LinearLayout column = new LinearLayout(getContext()); column.setOrientation(VERTICAL); return column; }
    private LayoutParams gapParams(int width, int height, int bottom) { LayoutParams p = new LayoutParams(width, height); p.bottomMargin = dp(bottom); return p; }
    private Button tab(String text, boolean target) {
        Button button = ui.button(text, () -> select(target), false); button.setTextSize(13); button.setPadding(dp(6), dp(9), dp(6), dp(9)); return button;
    }
    private LinearLayout card(LinearLayout parent, int padding) {
        LinearLayout card = vertical(); card.setPadding(dp(padding), dp(padding), dp(padding), dp(padding));
        card.setBackground(ui.shape(ui.surface, 24)); parent.addView(card, gapParams(-1, -2, 12)); return card;
    }
    private ImageView icon(String name, int color, int size) {
        ImageView icon = new ImageView(getContext()); icon.setImageDrawable(ui.icon(name, color));
        icon.setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_NO); icon.setLayoutParams(new LayoutParams(dp(size), dp(size))); return icon;
    }
    private List<Bundle> notifications() {
        ArrayList<Bundle> values = state.getParcelableArrayList("notifications");
        if (values == null) return Collections.emptyList();
        ArrayList<Bundle> sorted = new ArrayList<>(values); sorted.sort((a,b) -> Long.compare(b.getLong("posted"), a.getLong("posted"))); return sorted;
    }
    private List<Bundle> actions(Bundle notice) {
        ArrayList<Bundle> values = notice.getParcelableArrayList("actions"); return values == null ? Collections.emptyList() : values;
    }
    private boolean hasAction(String handle) {
        for (Bundle notice : notifications()) for (Bundle action : actions(notice)) if (handle.equals(action.getString("handle"))) return true;
        return false;
    }
    private int signature() {
        int result = Objects.hash(controls, settingsPage, state.getString("network_summary"), state.getFloat("brightness"), state.getFloat("volume"),
                state.getBoolean("brightness_automatic"), state.getInt("interruption_filter"), state.getBoolean("battery_saver"));
        for (String op : new String[]{"wifi", "bluetooth", "torch", "rotation_locked", "notifications", "brightness", "volume", "rotation"})
            result = 31 * result + Objects.hash(state.getBoolean(op), owner.accessible(op));
        if (!controls) for (Bundle notice : notifications()) {
            result = 31 * result + Objects.hash(notice.getString("handle"), notice.getString("app_label"), notice.getString("title"), notice.getString("text"), notice.getLong("posted"), notice.getBoolean("dismissible"));
            for (Bundle action : actions(notice)) result = 31 * result + Objects.hash(action.getString("handle"), action.getString("label"), action.getBoolean("reply"));
        }
        return result;
    }
    void update(Bundle value) {
        state = value;
        if (replyHandle != null) {
            if (!owner.accessible("notifications") || !hasAction(replyHandle)) { clearReply(); message("Notification changed. Open its latest action to reply."); }
            else return; // Keep the draft and keyboard stable until its authority changes.
        }
        if (!sliding && signature() != renderedSignature) render();
    }
    private void select(boolean target) { clearReply(); controls = target; settingsPage = false; scroll.scrollTo(0,0); message(""); render(); }
    private void render() {
        int position = scroll.getScrollY(); body.removeAllViews(); renderedSignature = signature();
        title.setText(settingsPage ? "Settings" : controls ? "Control center" : "Notifications");
        title.setTextSize(wide ? 25 : 32); date.setVisibility(wide || settingsPage ? GONE : VISIBLE);
        setup.setVisibility(wide || settingsPage ? GONE : VISIBLE);
        setContentDescription(settingsPage ? "OctoSense Settings" : controls ? "OctoSense Controls" : "OctoSense Notifications");
        back.setVisibility(settingsPage ? VISIBLE : GONE); tabs.setVisibility(settingsPage ? GONE : VISIBLE);
        notificationsTab.setSelected(!controls); controlsTab.setSelected(controls);
        notificationsTab.setStateDescription(!controls ? "Selected" : ""); controlsTab.setStateDescription(controls ? "Selected" : "");
        int selected = ui.dark ? 0xff30443d : ui.surface;
        notificationsTab.setBackground(ui.touch(!controls ? selected : 0x00000000, 16));
        controlsTab.setBackground(ui.touch(controls ? selected : 0x00000000, 16));
        notificationsTab.setTextColor(!controls ? ui.text : ui.muted); controlsTab.setTextColor(controls ? ui.text : ui.muted);
        if (settingsPage) settings(); else if (controls) controls(); else notificationsPage();
        scroll.post(() -> scroll.scrollTo(0, position));
    }
    private void controls() {
        LinearLayout first = body, second = body;
        if (wide) {
            LinearLayout columns = horizontal(); columns.setGravity(Gravity.TOP); first = vertical(); second = vertical();
            LayoutParams a = new LayoutParams(0,-2,1); a.setMarginEnd(dp(12)); columns.addView(first,a); columns.addView(second,new LayoutParams(0,-2,1)); body.addView(columns);
        }
        LinearLayout network = horizontal(); network.setPadding(dp(4), 0, dp(4), dp(12));
        network.addView(icon("wifi", ui.accent, 20));
        TextView status = ui.label(state.getString("network_summary", "Set up your connection"), 13, ui.muted); status.setPaddingRelative(dp(8), 0, dp(8), 0);
        network.addView(status,new LayoutParams(0,-2,1)); network.addView(icon("chevron",ui.muted,16)); network.setMinimumHeight(dp(48));
        network.setBackground(ui.touch(0x00000000,12)); network.setFocusable(true); network.setContentDescription("Your connection. " + status.getText());
        network.setOnClickListener(v -> owner.settings("internet")); first.addView(network);
        tileRow(first, tile("Wi-Fi", "wifi", "wifi", "wifi"), tile("Bluetooth", "bluetooth", "bluetooth", "bluetooth"));
        tileRow(first, tile("Flashlight", "torch", "torch", "torch"), tile("Rotation lock", "rotation", "rotation_locked", "rotation"));
        LinearLayout sliders = card(second, 14);
        slider(sliders, "Brightness", "brightness", "sun", state.getFloat("brightness", .5f));
        View divider = new View(getContext()); divider.setBackgroundColor(ui.outline); LayoutParams line = gapParams(-1,dp(1),5); line.topMargin=dp(2); sliders.addView(divider,line);
        slider(sliders, "Media volume", "volume", "volume", state.getFloat("volume", .5f));
        boolean quiet = state.getInt("interruption_filter",1) > 1;
        tileRow(second, routeTile("Do Not Disturb",quiet ? "On · Settings" : "Off · Settings","moon",quiet,"dnd"),
                routeTile("Battery saver", state.getBoolean("battery_saver") ? "On · Settings" : "Off · Settings", "battery",state.getBoolean("battery_saver"),"battery"));
    }
    private void tileRow(LinearLayout parent, View first, View second) {
        LinearLayout row = horizontal(); row.setGravity(Gravity.TOP); LayoutParams left = new LayoutParams(0,-1,1); left.setMarginEnd(dp(10));
        row.addView(first,left); row.addView(second,new LayoutParams(0,-1,1)); parent.addView(row,gapParams(-1,-2,10));
    }
    private LinearLayout tile(String name, String operation, String key, String symbol) {
        boolean on = state.getBoolean(key), direct = owner.accessible(operation);
        boolean known = !operation.equals("bluetooth") || state.containsKey("bluetooth");
        String status = (known ? (on ? "On" : "Off") : "") + (direct ? "" : (known ? " · " : "") + (operation.equals("wifi") || operation.equals("bluetooth") ? "Settings" : "Set up"));
        LinearLayout tile = tileView(name,status,symbol,on && known);
        tile.setStateDescription(status);
        tile.setAccessibilityDelegate(new AccessibilityDelegate() {
            @Override public void onInitializeAccessibilityNodeInfo(View host, android.view.accessibility.AccessibilityNodeInfo info) {
                super.onInitializeAccessibilityNodeInfo(host,info);
                info.setClassName(direct ? "android.widget.Switch" : "android.widget.Button");
                info.setCheckable(direct); info.setChecked(on);
            }
        });
        tile.setOnClickListener(v -> {
            if (!owner.accessible(operation)) owner.settings(operation);
            else { owner.command(operation, !on, 0, null, null); v.setEnabled(false); v.postDelayed(() -> v.setEnabled(true), 1500); }
        });
        tile.setOnLongClickListener(v -> { owner.settings(operation); return true; }); return tile;
    }
    private LinearLayout routeTile(String name, String subtitle, String symbol, boolean active, String route) {
        LinearLayout tile = horizontal(); tile.setPadding(dp(14),dp(12),dp(12),dp(12)); tile.setMinimumHeight(dp(wide ? 64 : 76));
        tile.setBackground(ui.touch(active ? ui.accent : ui.surface,22)); int ink=active ? ui.onAccent : ui.text;
        tile.addView(icon(symbol,ink,23)); LinearLayout words=vertical(); words.setPaddingRelative(dp(10),0,0,0);
        TextView label=ui.label(name,13,ink); ui.medium(label); words.addView(label);
        TextView status=ui.label(subtitle,11,active ? ui.onAccent : ui.muted); status.setPadding(0,dp(4),0,0); words.addView(status);
        tile.addView(words,new LayoutParams(0,-2,1)); tile.setFocusable(true); tile.setContentDescription(name+". "+subtitle);
        for(int i=0;i<tile.getChildCount();i++) tile.getChildAt(i).setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS);
        tile.setOnClickListener(v -> owner.settings(route)); return tile;
    }
    private LinearLayout tileView(String name, String subtitle, String symbol, boolean active) {
        LinearLayout tile = vertical(); tile.setPadding(dp(16),dp(12),dp(16),dp(12)); tile.setMinimumHeight(dp(94));
        tile.setBackground(ui.touch(active ? ui.accent : ui.surface,24)); tile.setFocusable(true);
        int ink = active ? ui.onAccent : ui.text;
        LinearLayout top = horizontal(); top.addView(icon(symbol,ink,23)); View space = new View(getContext()); top.addView(space,new LayoutParams(0,1,1));
        if (subtitle.contains("Settings") || subtitle.contains("Set up")) top.addView(icon("chevron",ink,15)); tile.addView(top);
        TextView label = ui.label(name,15,ink); ui.medium(label); label.setPadding(0,dp(9),0,dp(4)); tile.addView(label);
        TextView value = ui.label(subtitle,12,active ? ui.onAccent : ui.muted); tile.addView(value);
        tile.setContentDescription(name + ". " + subtitle); tile.setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_YES);
        for (int i=0;i<tile.getChildCount();i++) tile.getChildAt(i).setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS);
        return tile;
    }
    private void slider(LinearLayout box, String name, String operation, String symbol, float value) {
        boolean enabled = owner.accessible(operation), automatic = operation.equals("brightness") && state.getBoolean("brightness_automatic");
        LinearLayout heading = horizontal(); heading.addView(icon(symbol,ui.muted,20));
        TextView label = ui.label(name,14,ui.text); ui.medium(label); label.setPaddingRelative(dp(8),0,0,0); heading.addView(label,new LayoutParams(0,-2,1));
        TextView amount = ui.label(enabled ? automatic ? "Auto" : Math.round(value*100)+"%" : "Set up",12,ui.muted); amount.setPaddingRelative(dp(8),0,0,0); heading.addView(amount); box.addView(heading);
        if (!enabled || automatic) {
            Button setup = ui.button(automatic ? "Automatic brightness · Display settings" : "Allow " + name.toLowerCase(), () -> owner.settings(automatic ? "display" : operation), false);
            setup.setTextSize(12); box.addView(setup,gapParams(-1,-2,0)); return;
        }
        SeekBar seek = new SeekBar(getContext()); seek.setMax(100); seek.setProgress(Math.round(Math.max(0,Math.min(1,value))*100));
        seek.setContentDescription(name); seek.setPadding(dp(10),0,dp(10),0); seek.setSplitTrack(false);
        // SeekBar mirrors its canvas in RTL; a START clip would mirror twice.
        LayerDrawable track = new LayerDrawable(new Drawable[]{ui.shape(ui.elevated,6),new ClipDrawable(ui.shape(ui.accent,6),Gravity.LEFT,ClipDrawable.HORIZONTAL)});
        track.setId(0,android.R.id.background); track.setId(1,android.R.id.progress); track.setLayerHeight(0,dp(10)); track.setLayerHeight(1,dp(10));
        track.setLayerGravity(0,Gravity.CENTER_VERTICAL); track.setLayerGravity(1,Gravity.CENTER_VERTICAL); seek.setProgressDrawable(track);
        android.graphics.drawable.GradientDrawable thumb=ui.shape(ui.accent,12); thumb.setSize(dp(22),dp(22)); seek.setThumb(thumb); seek.setThumbOffset(dp(11));
        box.addView(seek,new LayoutParams(-1,dp(48)));
        seek.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
            @Override public void onStartTrackingTouch(SeekBar s) { sliding = true; }
            @Override public void onProgressChanged(SeekBar s,int progress,boolean user) { if(user) amount.setText(progress+"%"); }
            @Override public void onStopTrackingTouch(SeekBar s) { sliding=false; owner.command(operation,false,s.getProgress()/100f,null,null); }
        });
    }
    private void settings() {
        section("CONNECTIONS");
        route("Wi-Fi networks", "Connect to a network", "wifi", "wifi");
        route("Pair Bluetooth", "Headphones, speakers & devices", "bluetooth", "bluetooth");
        route("Mobile network", "SIM, data & roaming", "mobile", "mobile");
        route("Hotspot", "Share your connection", "hotspot", "hotspot");
        route("VPN", "Private network connections", "shield", "vpn");
        section("YOUR DEVICE");
        route("Display", "Brightness, text & sleep", "sun", "display");
        route("Sound", "Volume, vibration & ringtones", "volume", "sound");
        route("Accessibility", "Make your phone work for you", "accessibility", "accessibility");
        section("OCTOSENSE");
        route("Permissions", "Notifications & device controls", "shield", "access");
        route("Panel across apps", "Choose your pull-down panel", "grid", "shade");
    }
    private void section(String name) { TextView text=ui.label(name,11,ui.muted); text.setAccessibilityHeading(true); text.setLetterSpacing(.1f); text.setPadding(dp(4),dp(8),0,dp(12)); body.addView(text); }
    private void route(String name,String detail,String symbol,String destination) {
        LinearLayout row=horizontal(); row.setPadding(dp(16),dp(14),dp(14),dp(14)); row.setMinimumHeight(dp(72)); row.setBackground(ui.touch(ui.surface,20));
        row.addView(icon(symbol,ui.accent,23)); LinearLayout words=vertical(); words.setPaddingRelative(dp(14),0,dp(8),0);
        TextView label=ui.label(name,15,ui.text); ui.medium(label); words.addView(label);
        TextView sub=ui.label(detail,12,ui.muted); sub.setPadding(0,dp(4),0,0); words.addView(sub); row.addView(words,new LayoutParams(0,-2,1)); row.addView(icon("chevron",ui.muted,18));
        row.setFocusable(true); row.setOnClickListener(v -> owner.settings(destination)); body.addView(row,gapParams(-1,-2,8));
    }
    private void empty(String symbol,String heading,String detail,String action,Runnable onAction) {
        LinearLayout box=card(body,28); box.setGravity(Gravity.CENTER); box.setPadding(dp(28),dp(42),dp(28),dp(42));
        ImageView mark=icon(symbol,ui.accent,48); mark.setBackground(ui.shape(ui.elevated,24)); mark.setPadding(dp(11),dp(11),dp(11),dp(11)); box.addView(mark);
        TextView title=ui.label(heading,23,ui.text); ui.medium(title); title.setGravity(Gravity.CENTER); title.setTextAlignment(TEXT_ALIGNMENT_CENTER); title.setPadding(0,dp(22),0,dp(10)); box.addView(title);
        TextView text=ui.label(detail,14,ui.muted); text.setGravity(Gravity.CENTER); text.setTextAlignment(TEXT_ALIGNMENT_CENTER); text.setLineSpacing(dp(3),1); box.addView(text);
        if(action!=null) { Button b=ui.button(action,onAction,true); LayoutParams p=new LayoutParams(-1,-2); p.topMargin=dp(24); box.addView(b,p); }
    }
    private void notificationsPage() {
        if(!owner.accessible("notifications")) { empty("bell","Your day, in one place","Allow notification access to read and respond here.","Enable notification access",() -> owner.settings("notifications")); return; }
        List<Bundle> notices=notifications();
        if(notices.isEmpty()) { empty("check","All caught up","New notifications will appear here.\nEnjoy a little quiet.",null,null); return; }
        LinearLayout summary=horizontal(); TextView count=ui.label(notices.size()+(notices.size()==1 ? " notification" : " notifications"),12,ui.muted); summary.addView(count,new LayoutParams(0,-2,1));
        boolean clear=false; for(Bundle notice:notices) clear|=notice.getBoolean("dismissible");
        if(clear) { Button b=ui.button("Clear all",() -> owner.command("dismiss_all",false,0,null,null),false); b.setContentDescription("Clear dismissible notifications"); b.setTextSize(12); b.setTextColor(ui.accent); b.setBackground(ui.touch(0x00000000,12)); summary.addView(b); }
        body.addView(summary,gapParams(-1,-2,8));
        for(Bundle notice:notices) if(notice.getBoolean("dismissible")) noticeCard(notice);
        boolean ongoing=false; for(Bundle notice:notices) if(!notice.getBoolean("dismissible")) { if(!ongoing) section("ONGOING"); ongoing=true; noticeCard(notice); }
    }
    private void noticeCard(Bundle notice) {
        LinearLayout box=card(body,16); String app=notice.getString("app_label","Notification");
        LinearLayout meta=horizontal();
        android.graphics.Bitmap appIcon=notice.getParcelable("app_icon",android.graphics.Bitmap.class);
        if(appIcon!=null) {
            ImageView image=new ImageView(getContext()); image.setImageBitmap(appIcon); image.setBackground(ui.shape(ui.elevated,10));
            image.setClipToOutline(true); image.setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_NO); meta.addView(image,new LayoutParams(dp(32),dp(32)));
        } else {
            TextView monogram=ui.label(app.isEmpty()?"N":app.substring(0,app.offsetByCodePoints(0,1)).toUpperCase(java.util.Locale.ROOT),13,ui.accent);
            ui.medium(monogram); monogram.setGravity(Gravity.CENTER); monogram.setTextAlignment(TEXT_ALIGNMENT_CENTER); monogram.setBackground(ui.shape(ui.elevated,10)); meta.addView(monogram,new LayoutParams(dp(32),dp(32)));
        }
        LinearLayout words=vertical(); words.setPaddingRelative(dp(10),0,dp(4),0); TextView label=ui.label(app,12,ui.muted); ui.medium(label); words.addView(label);
        long posted=notice.getLong("posted"); if(posted>0) { TextView time=ui.label(DateUtils.getRelativeTimeSpanString(posted,System.currentTimeMillis(),DateUtils.MINUTE_IN_MILLIS,DateUtils.FORMAT_ABBREV_RELATIVE).toString(),11,ui.muted); time.setPadding(0,dp(3),0,0); words.addView(time); }
        meta.addView(words,new LayoutParams(0,-2,1));
        if(notice.getBoolean("dismissible")) { Button clear=ui.button("Clear",() -> owner.command("dismiss",false,0,notice.getString("handle"),null),false); clear.setTextSize(12); clear.setTextColor(ui.muted); clear.setBackground(ui.touch(0x00000000,12)); meta.addView(clear); }
        box.addView(meta);
        TextView heading=ui.label(notice.getString("title",""),18,ui.text); ui.medium(heading); heading.setPadding(0,dp(10),0,dp(6)); box.addView(heading);
        String text=notice.getString("text",""); String identity=notice.getString("identity",notice.getString("handle",""));
        TextView content=ui.label(text,14,ui.muted); content.setLineSpacing(dp(2),1); content.setMaxLines(expanded.contains(identity)?Integer.MAX_VALUE:4); content.setEllipsize(TextUtils.TruncateAt.END); box.addView(content);
        if(text.length()>160) { Button more=ui.button(expanded.contains(identity)?"Show less":"Read more",() -> { if(!expanded.add(identity)) expanded.remove(identity); render(); },false); more.setTextSize(12); box.addView(more); }
        LinearLayout actions=vertical();
        for(Bundle action:actions(notice)) {
            String handle=action.getString("handle","");
            if(action.getBoolean("open")) { box.setFocusable(true); box.setBackground(ui.touch(ui.surface,24)); box.setOnClickListener(v -> { owner.command("action",false,0,handle,null); owner.dismiss(); }); }
            else { Button invoke=ui.button(action.getString("label","Open"),() -> { if(action.getBoolean("reply")) reply(notice,action); else owner.command("action",false,0,handle,null); },false); invoke.setTextColor(ui.accent); LayoutParams p=new LayoutParams(-1,-2); p.topMargin=dp(12); actions.addView(invoke,p); }
        }
        box.addView(actions);
    }
    private void reply(Bundle notice,Bundle action) {
        String handle=action.getString("handle",""); if(!hasAction(handle)) { message("This notification has changed."); return; }
        clearReply(); replyHandle=handle; replyCommand=-1; body.removeAllViews(); scroll.scrollTo(0,0);
        title.setText("Reply"); title.setTextSize(26); date.setVisibility(GONE); tabs.setVisibility(GONE); setup.setVisibility(GONE); back.setVisibility(VISIBLE);
        LinearLayout box=card(body,20); TextView to=ui.label("REPLY TO "+notice.getString("app_label","notification").toUpperCase(java.util.Locale.ROOT),11,ui.accent); to.setLetterSpacing(.1f); box.addView(to);
        TextView heading=ui.label(notice.getString("title",""),22,ui.text); ui.medium(heading); heading.setPadding(0,dp(12),0,dp(8)); box.addView(heading);
        TextView context=ui.label(notice.getString("text",""),14,ui.muted); context.setMaxLines(3); box.addView(context,gapParams(-1,-2,18));
        editor=new EditText(getContext()); editor.setTextColor(ui.text); editor.setHintTextColor(ui.muted); editor.setHint("Your reply"); editor.setTextSize(16);
        editor.setBackground(ui.shape(ui.elevated,18)); editor.setPadding(dp(16),dp(14),dp(16),dp(14)); editor.setMinLines(2); editor.setMaxLines(6); editor.setSaveEnabled(false);
        editor.setFilters(new InputFilter[]{new InputFilter.LengthFilter(2000)});
        editor.setInputType(android.text.InputType.TYPE_CLASS_TEXT | android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE | android.text.InputType.TYPE_TEXT_FLAG_CAP_SENTENCES);
        editor.setImeOptions(android.view.inputmethod.EditorInfo.IME_FLAG_NO_EXTRACT_UI);
        box.addView(editor,gapParams(-1,-2,12));
        LinearLayout actions=replyBar; actions.removeAllViews(); actions.setVisibility(VISIBLE);
        LayoutParams half=new LayoutParams(0,-2,1); half.setMarginEnd(dp(10));
        actions.addView(ui.button("Cancel",() -> { clearReply(); render(); },false),half);
        send=ui.button("Send",() -> {
            if(replyCommand>0 || editor==null || editor.getText().toString().trim().isEmpty()) return;
            if(!hasAction(replyHandle)) { clearReply(); message("This notification has changed."); render(); return; }
            replyCommand=owner.command("action",false,0,replyHandle,editor.getText().toString());
            if(replyCommand>0) { send.setEnabled(false); editor.setEnabled(false); message("Sending…"); }
        },true); actions.addView(send,new LayoutParams(0,-2,1));
        send.setEnabled(false); send.setAlpha(.45f);
        editor.addTextChangedListener(new android.text.TextWatcher() {
            @Override public void beforeTextChanged(CharSequence s,int start,int count,int after) { }
            @Override public void onTextChanged(CharSequence s,int start,int before,int count) {
                if(send!=null && replyCommand<0) { boolean ready=s.toString().trim().length()>0; send.setEnabled(ready); send.setAlpha(ready?1:.45f); }
            }
            @Override public void afterTextChanged(android.text.Editable value) { }
        });
        editor.requestFocus(); editor.post(() -> { if(editor!=null) getContext().getSystemService(InputMethodManager.class).showSoftInput(editor,InputMethodManager.SHOW_IMPLICIT); });
    }
    void result(long id,int status) {
        if(status==Protocol.ACCEPTED) return;
        if(id==replyCommand) {
            if(status==Protocol.COMPLETED) { clearReply(); render(); message("Reply sent"); }
            else message(status==Protocol.EXPIRED_HANDLE ? "Notification changed. Open its latest action to reply." : "Could not confirm delivery. Check the conversation before sending again.");
        } else if(status!=Protocol.COMPLETED) message("Could not complete that action. Try its settings or latest notification.");
    }
    void message(String value) { feedback.setText(value); feedback.setVisibility(value.isEmpty()?GONE:VISIBLE); }
    private void clearReply() {
        if(editor!=null) { getContext().getSystemService(InputMethodManager.class).hideSoftInputFromWindow(getWindowToken(),0); editor.setText(""); editor.clearFocus(); }
        editor=null; send=null; replyHandle=null; replyCommand=-1;
        replyBar.removeAllViews(); replyBar.setVisibility(GONE);
    }
    void clearPrivateState() { clearReply(); expanded.clear(); state=Bundle.EMPTY; body.removeAllViews(); }
    @Override public boolean dispatchKeyEvent(KeyEvent event) {
        if(event.getKeyCode()==KeyEvent.KEYCODE_BACK) {
            if(event.getAction()==KeyEvent.ACTION_UP) {
                if(replyHandle!=null) { clearReply(); render(); }
                else if(settingsPage) { settingsPage=false; scroll.scrollTo(0,0); render(); }
                else owner.dismiss();
            }
            return true;
        }
        return super.dispatchKeyEvent(event);
    }
}
