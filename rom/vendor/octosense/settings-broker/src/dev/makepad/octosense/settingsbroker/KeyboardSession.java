package dev.makepad.octosense.settingsbroker;

import android.content.Context;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import dev.makepad.octosense.keyboards.KeyboardPolicy;
import java.util.concurrent.Callable;
import java.util.concurrent.FutureTask;
import java.util.concurrent.TimeUnit;

/** Main-thread native model shared by the service and its unexported one-shot Activity. */
final class KeyboardSession {
    private static KeyboardSession instance;
    final KeyboardPlatform platform;
    final KeyboardPolicy policy;
    private KeyboardSession(Context context){platform=new KeyboardPlatform(context);policy=new KeyboardPolicy(platform,SystemClock::elapsedRealtime);}
    static KeyboardSession get(Context context){if(Looper.myLooper()!=Looper.getMainLooper())throw new IllegalStateException("Keyboard session requires main thread");if(instance==null)instance=new KeyboardSession(context);return instance;}
    static <T> T onMain(Callable<T> operation)throws Exception{
        if(Looper.myLooper()==Looper.getMainLooper())return operation.call();
        FutureTask<T> task=new FutureTask<>(operation);Handler main=new Handler(Looper.getMainLooper());main.post(task);
        try{return task.get(5,TimeUnit.SECONDS);}finally{if(!task.isDone()){task.cancel(false);main.removeCallbacks(task);}}
    }
}
