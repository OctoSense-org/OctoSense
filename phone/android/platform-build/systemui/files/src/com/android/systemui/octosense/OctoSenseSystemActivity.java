/* Copyright 2026 OctoSense. Licensed under the Apache License, Version 2.0. */
package com.android.systemui.octosense;

import android.app.Activity;
import android.app.KeyguardManager;
import android.content.ActivityNotFoundException;
import android.content.BroadcastReceiver;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.res.ColorStateList;
import android.database.ContentObserver;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.media.AudioManager;
import android.net.ConnectivityManager;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.provider.Settings;
import android.view.Gravity;
import android.view.View;
import android.view.WindowInsets;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.SeekBar;
import android.widget.Switch;
import android.widget.TextView;
import android.widget.Toast;
import com.android.systemui.res.R;

/** Unlocked, native SystemUI controls. No credential handling or external command surface. */
public final class OctoSenseSystemActivity extends Activity {
    private final Handler main = new Handler(Looper.getMainLooper());
    private LinearLayout content;
    private TextView connection, brightnessLabel;
    private SeekBar brightness, volume;
    private Switch rotation;
    private AudioManager audio;
    private ConnectivityManager connectivity;
    private boolean observing, networkObserving, updating;
    private final ContentObserver settings = new ContentObserver(main) {
        @Override public void onChange(boolean selfChange) { refresh(); }
    };
    private final ConnectivityManager.NetworkCallback network = new ConnectivityManager.NetworkCallback() {
        @Override public void onAvailable(Network n) { main.post(OctoSenseSystemActivity.this::refresh); }
        @Override public void onLost(Network n) { main.post(OctoSenseSystemActivity.this::refresh); }
        @Override public void onCapabilitiesChanged(Network n, NetworkCapabilities c) { main.post(OctoSenseSystemActivity.this::refresh); }
    };
    private final BroadcastReceiver events = new BroadcastReceiver() {
        @Override public void onReceive(Context c, Intent intent) {
            if (Intent.ACTION_SCREEN_OFF.equals(intent.getAction())) finish(); else refresh();
        }
    };
    @Override public void onCreate(Bundle saved) {
        setTheme(R.style.Theme_OctoSense_System);
        super.onCreate(saved);
        setTitle(R.string.octosense_device);
        audio = getSystemService(AudioManager.class);
        connectivity = getSystemService(ConnectivityManager.class);
        LinearLayout root = new LinearLayout(this); root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(getColor(R.color.octosense_surface));
        root.setOnApplyWindowInsetsListener((v, insets) -> {
            android.graphics.Insets safe = insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
            v.setPadding(dp(20) + safe.left, dp(16) + safe.top, dp(20) + safe.right, dp(12) + safe.bottom);
            return insets;
        });
        LinearLayout heading = new LinearLayout(this); heading.setGravity(Gravity.CENTER_VERTICAL);
        LinearLayout words = new LinearLayout(this); words.setOrientation(LinearLayout.VERTICAL);
        TextView brand = label(R.string.octosense_brand, 12); brand.setLetterSpacing(.16f); brand.setTextColor(getColor(R.color.octosense_accent));
        words.addView(brand); TextView title = label(R.string.octosense_device, 30); title.setTypeface(null, Typeface.BOLD); words.addView(title);
        heading.addView(words, new LinearLayout.LayoutParams(0, -2, 1)); heading.addView(button(R.string.octosense_close, this::finish)); root.addView(heading);
        ScrollView scroll = new ScrollView(this); content = new LinearLayout(this); content.setOrientation(LinearLayout.VERTICAL); scroll.addView(content);
        root.addView(scroll, new LinearLayout.LayoutParams(-1, 0, 1));
        LinearLayout networkCard = card(); networkCard.addView(label(R.string.octosense_connection, 14));
        connection = label(R.string.octosense_checking, 19); networkCard.addView(connection);
        networkCard.setFocusable(true); networkCard.setOnClickListener(v -> open(Settings.Panel.ACTION_INTERNET_CONNECTIVITY));
        LinearLayout light = card(); brightnessLabel = label(R.string.octosense_brightness, 16); light.addView(brightnessLabel);
        brightness = slider(light, R.string.octosense_brightness, 255, value -> {
            if (!Settings.System.canWrite(this) || Settings.System.getInt(getContentResolver(), Settings.System.SCREEN_BRIGHTNESS_MODE, 0) != 0) return;
            if (!Settings.System.putInt(getContentResolver(), Settings.System.SCREEN_BRIGHTNESS, Math.max(1, value))) failed();
        });
        light.setOnClickListener(v -> open(Settings.ACTION_DISPLAY_SETTINGS));
        LinearLayout sound = card(); sound.addView(label(R.string.octosense_volume, 16));
        volume = slider(sound, R.string.octosense_volume, audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC),
                value -> audio.setStreamVolume(AudioManager.STREAM_MUSIC, value, 0));
        LinearLayout turn = card(); rotation = new Switch(this); rotation.setText(R.string.octosense_rotation); rotation.setTextColor(getColor(R.color.octosense_text));
        rotation.setOnCheckedChangeListener((view, checked) -> {
            if (updating) return;
            if (getSystemService(KeyguardManager.class).isKeyguardLocked()) { finish(); return; }
            try {
                if (!Settings.System.putInt(getContentResolver(), Settings.System.ACCELEROMETER_ROTATION, checked ? 0 : 1)) failed();
            } catch (SecurityException denied) { failed(); }
            refresh();
        }); turn.addView(rotation);
        routes(R.string.octosense_wifi, Settings.ACTION_WIFI_SETTINGS, R.string.octosense_bluetooth, Settings.ACTION_BLUETOOTH_SETTINGS);
        routes(R.string.octosense_mobile, Settings.ACTION_WIRELESS_SETTINGS, R.string.octosense_hotspot, "android.settings.TETHER_SETTINGS");
        routes(R.string.octosense_vpn, Settings.ACTION_VPN_SETTINGS, R.string.octosense_battery, Settings.ACTION_BATTERY_SAVER_SETTINGS);
        routes(R.string.octosense_display, Settings.ACTION_DISPLAY_SETTINGS, R.string.octosense_sound, Settings.ACTION_SOUND_SETTINGS);
        routes(R.string.octosense_dnd, "android.settings.ZEN_MODE_SETTINGS", R.string.octosense_accessibility, Settings.ACTION_ACCESSIBILITY_SETTINGS);
        root.addView(button(R.string.octosense_setup, () -> start(new Intent().setComponent(new ComponentName(
                "dev.makepad.octosense.bridge", "dev.makepad.octosense.bridge.BridgeSettingsActivity")))));
        setContentView(root);
    }
    @Override protected void onResume() {
        super.onResume();
        if (getSystemService(KeyguardManager.class).isKeyguardLocked()) { finish(); return; }
        getContentResolver().registerContentObserver(Settings.System.CONTENT_URI, true, settings);
        IntentFilter filter = new IntentFilter("android.media.VOLUME_CHANGED_ACTION"); filter.addAction(Intent.ACTION_SCREEN_OFF);
        registerReceiver(events, filter, Context.RECEIVER_EXPORTED); observing = true;
        try { connectivity.registerDefaultNetworkCallback(network); networkObserving = true; }
        catch (RuntimeException unavailable) { networkObserving = false; }
        refresh();
    }
    @Override protected void onPause() {
        if (observing) { getContentResolver().unregisterContentObserver(settings); unregisterReceiver(events); observing = false; }
        if (networkObserving) { connectivity.unregisterNetworkCallback(network); networkObserving = false; }
        main.removeCallbacksAndMessages(null);
        super.onPause();
    }
    private void refresh() {
        if (isFinishing() || isDestroyed() || brightness == null) return;
        updating = true;
        try {
            boolean automatic = Settings.System.getInt(getContentResolver(), Settings.System.SCREEN_BRIGHTNESS_MODE, 0) != 0;
            brightnessLabel.setText(automatic ? R.string.octosense_auto_brightness : R.string.octosense_brightness);
            brightness.setEnabled(!automatic && Settings.System.canWrite(this));
            if (!brightness.isPressed()) brightness.setProgress(Settings.System.getInt(getContentResolver(), Settings.System.SCREEN_BRIGHTNESS, 128));
            if (!volume.isPressed()) volume.setProgress(audio.getStreamVolume(AudioManager.STREAM_MUSIC));
            rotation.setEnabled(Settings.System.canWrite(this));
            rotation.setChecked(Settings.System.getInt(getContentResolver(), Settings.System.ACCELEROMETER_ROTATION, 1) == 0);
            NetworkCapabilities capabilities = connectivity.getNetworkCapabilities(connectivity.getActiveNetwork());
            connection.setText(capabilities == null ? R.string.octosense_offline
                    : capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_CAPTIVE_PORTAL) ? R.string.octosense_sign_in
                    : capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) ? R.string.octosense_connected : R.string.octosense_no_internet);
        } finally { updating = false; }
    }
    private interface Change { void apply(int value); }
    private SeekBar slider(LinearLayout box, int name, int max, Change action) {
        SeekBar seek = new SeekBar(this); seek.setMax(max); seek.setContentDescription(getString(name));
        seek.setProgressTintList(ColorStateList.valueOf(getColor(R.color.octosense_accent)));
        seek.setThumbTintList(ColorStateList.valueOf(getColor(R.color.octosense_accent))); box.addView(seek, new LinearLayout.LayoutParams(-1, dp(48)));
        seek.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
            @Override public void onStartTrackingTouch(SeekBar s) { }
            @Override public void onProgressChanged(SeekBar s, int progress, boolean user) { }
            @Override public void onStopTrackingTouch(SeekBar s) {
                if (getSystemService(KeyguardManager.class).isKeyguardLocked()) { finish(); return; }
                try { action.apply(s.getProgress()); } catch (SecurityException | IllegalArgumentException denied) { failed(); }
                refresh();
            }
        }); return seek;
    }
    private int dp(int value) { return Math.round(value * getResources().getDisplayMetrics().density); }
    private TextView label(int text, int size) { TextView view = new TextView(this); view.setText(text); view.setTextSize(size); view.setTextColor(getColor(R.color.octosense_text)); return view; }
    private Button button(int text, Runnable action) { Button b = new Button(this); b.setText(text); b.setAllCaps(false); b.setTextColor(getColor(R.color.octosense_text)); b.setBackgroundTintList(ColorStateList.valueOf(getColor(R.color.octosense_card))); b.setOnClickListener(v -> action.run()); return b; }
    private LinearLayout card() {
        LinearLayout card = new LinearLayout(this); card.setOrientation(LinearLayout.VERTICAL); card.setPadding(dp(16), dp(16), dp(16), dp(16));
        GradientDrawable bg = new GradientDrawable(); bg.setColor(getColor(R.color.octosense_card)); bg.setCornerRadius(dp(22)); card.setBackground(bg);
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(-1, -2); params.setMargins(0, dp(7), 0, dp(7)); content.addView(card, params); return card;
    }
    private void routes(int left, String leftAction, int right, String rightAction) {
        LinearLayout row = new LinearLayout(this); row.addView(button(left, () -> open(leftAction)), new LinearLayout.LayoutParams(0, -2, 1));
        row.addView(button(right, () -> open(rightAction)), new LinearLayout.LayoutParams(0, -2, 1)); content.addView(row);
    }
    private void open(String action) { start(new Intent(action)); }
    private void start(Intent intent) {
        if (getSystemService(KeyguardManager.class).isKeyguardLocked()) { finish(); return; }
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TOP);
        try { startActivity(intent); }
        catch (ActivityNotFoundException | SecurityException unavailable) {
            try { startActivity(new Intent(Settings.ACTION_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TOP)); }
            catch (ActivityNotFoundException | SecurityException absent) { failed(); }
        }
    }
    private void failed() { Toast.makeText(this, R.string.octosense_action_failed, Toast.LENGTH_SHORT).show(); }
}
