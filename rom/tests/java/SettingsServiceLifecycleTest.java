package dev.makepad.octosense.agent;

import android.content.ComponentName;
import android.content.Context;
import android.content.ServiceConnection;
import android.os.Handler;
import android.os.IBinder;
import android.os.IInterface;
import android.view.MotionEvent;
import dev.makepad.octosense.ObscuredTouchGuard;

public final class SettingsServiceLifecycleTest {
    static final class Remote implements IInterface, IBinder {}
    static final ComponentName COMPONENT = new ComponentName("broker", "service");
    static void check(boolean value) {if (!value) throw new AssertionError();}
    static SettingsServiceConnection<Remote> client(Context context) {
        return new SettingsServiceConnection<>(context, COMPONENT, binder -> (Remote) binder);
    }
    public static void main(String[] args) {
        Context context = new Context();
        SettingsServiceConnection<Remote> client = client(context);
        Handler.drain(); check(context.binds == 0); // Agent startup never starts the broker.
        check(client.current() == null); Handler.drain(); check(context.binds == 1);
        Remote remote = new Remote();
        context.connection.onServiceConnected(COMPONENT, remote);
        check(client.current() == remote); Handler.drain();
        Handler.advance(SettingsServiceConnection.IDLE_MS - 1); check(context.unbinds == 0);
        check(client.current() == remote); Handler.drain(); // Activity renews the lease.
        Handler.advance(SettingsServiceConnection.IDLE_MS - 1); check(context.unbinds == 0);
        Handler.advance(1); check(context.unbinds == 1);
        check(client.current() == null); Handler.drain(); check(context.binds == 2);
        context.connection.onServiceConnected(COMPONENT, remote);
        context.connection.onBindingDied(COMPONENT); check(context.unbinds == 2);
        check(client.current() == null); Handler.drain(); check(context.binds == 2);
        Handler.advance(SettingsServiceConnection.RETRY_MS);
        client.current(); Handler.drain(); check(context.binds == 3);
        context.connection.onNullBinding(COMPONENT); check(context.unbinds == 3);
        Handler.advance(SettingsServiceConnection.RETRY_MS);
        client.current(); Handler.drain(); check(context.binds == 4);
        ServiceConnection late = context.connection;
        client.close(); check(client.current() == null); Handler.drain(); check(context.unbinds == 4);
        late.onServiceConnected(COMPONENT, remote); check(client.current() == null);
        Handler.advance(SettingsServiceConnection.IDLE_MS); check(context.unbinds == 4);

        Context denied = new Context(); denied.pm.signature = -1;
        SettingsServiceConnection<Remote> deniedClient = client(denied);
        deniedClient.current(); Handler.drain(); check(denied.binds == 0);
        deniedClient.close(); Handler.drain();
        Context unavailable = new Context(); unavailable.available = false;
        SettingsServiceConnection<Remote> unavailableClient = client(unavailable);
        unavailableClient.current(); Handler.drain(); check(unavailable.binds == 1);
        for (int i=0;i<100;i++) unavailableClient.current();
        Handler.drain(); check(unavailable.binds == 1);
        Handler.advance(SettingsServiceConnection.RETRY_MS); unavailable.available = true;
        unavailableClient.current(); Handler.drain(); check(unavailable.binds == 2);
        Handler.advance(SettingsServiceConnection.IDLE_MS); check(unavailable.unbinds == 1); // A hung bind expires too.
        unavailableClient.close(); Handler.drain();
        Context closing = new Context(); SettingsServiceConnection<Remote> closingClient = client(closing);
        closingClient.current(); closingClient.close(); Handler.drain(); check(closing.binds == 0);

        ObscuredTouchGuard touch = new ObscuredTouchGuard();
        check(touch.accept(new MotionEvent(0,0)));
        check(!touch.accept(new MotionEvent(2,1))); // Overlay arrives after down.
        check(!touch.accept(new MotionEvent(1,0))); // A clear release must not click.
        check(touch.accept(new MotionEvent(0,0))); check(touch.accept(new MotionEvent(1,0)));
        check(!touch.accept(new MotionEvent(0,2))); // Partially obscured window.
        check(!touch.accept(new MotionEvent(5,0))); // A second finger cannot reset it.
        check(!touch.accept(new MotionEvent(1,0)));
        check(touch.accept(new MotionEvent(0,0)));
    }
}
