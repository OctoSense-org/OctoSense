package dev.makepad.octosense.agent;

import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.pm.PackageManager;
import android.os.Handler;
import android.os.IBinder;
import android.os.IInterface;
import android.os.Looper;
import android.os.SystemClock;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.Function;

/** Lazy, signature-checked binding with a bounded idle lifetime. Never replays an operation. */
final class SettingsServiceConnection<T extends IInterface> implements AutoCloseable {
    static final long IDLE_MS = 30_000;
    static final long RETRY_MS = 5_000;
    private final Context context;
    private final ComponentName component;
    private final Function<IBinder,T> decode;
    private final Handler main = new Handler(Looper.getMainLooper());
    private final AtomicBoolean queued = new AtomicBoolean();
    private volatile T service;
    private volatile boolean closed;
    private volatile long lastUse;
    // Only the main looper owns the binding and its timers.
    private boolean bound;
    private long nextAttempt;
    private final Runnable idle = this::expire;

    SettingsServiceConnection(Context context, ComponentName component, Function<IBinder,T> decode) {
        this.context = context;
        this.component = component;
        this.decode = decode;
    }

    T current() {
        if (closed) return null;
        lastUse = SystemClock.elapsedRealtime();
        if (queued.compareAndSet(false, true)) main.post(() -> {
            queued.set(false);
            touch();
        });
        return closed ? null : service;
    }

    private void touch() {
        if (closed) return;
        long now = SystemClock.elapsedRealtime();
        if (!bound && now >= nextAttempt) {
            nextAttempt = now + RETRY_MS;
            if (context.getPackageManager().checkSignatures(context.getPackageName(),
                    component.getPackageName()) == PackageManager.SIGNATURE_MATCH) {
                try {
                    bound = context.bindService(new Intent().setComponent(component), connection,
                            Context.BIND_AUTO_CREATE);
                } catch (SecurityException unavailable) {
                    bound = false;
                }
            }
        }
        main.removeCallbacks(idle);
        if (bound) main.postDelayed(idle, Math.max(1, IDLE_MS - (now - lastUse)));
    }

    private void expire() {
        long remaining = IDLE_MS - (SystemClock.elapsedRealtime() - lastUse);
        if (!closed && remaining > 0) main.postDelayed(idle, remaining);
        else unbind();
    }

    private final ServiceConnection connection = new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name, IBinder binder) {
            if (!closed && bound) service = decode.apply(binder);
        }
        @Override public void onServiceDisconnected(ComponentName name) { service = null; }
        @Override public void onBindingDied(ComponentName name) { unbind(); }
        @Override public void onNullBinding(ComponentName name) { unbind(); }
    };

    private void unbind() {
        main.removeCallbacks(idle);
        service = null;
        if (bound) context.unbindService(connection);
        bound = false;
    }

    @Override public void close() {
        closed = true;
        service = null;
        main.post(this::unbind);
    }
}
