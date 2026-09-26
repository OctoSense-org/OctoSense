package dev.makepad.octosense.settingsa11yfixture;

import android.app.UiAutomation;
import android.os.ParcelFileDescriptor;
import android.os.SystemClock;
import android.view.InputDevice;
import java.io.IOException;
import java.nio.charset.StandardCharsets;

/** Disposable real InputReader source. Sends only relative motion, never clicks. */
final class KernelMouse implements AutoCloseable {
    private static final String NAME="OctoSense accessibility validation mouse";
    private final ParcelFileDescriptor[] pipes;
    private final ParcelFileDescriptor.AutoCloseOutputStream writer;
    KernelMouse(UiAutomation automation)throws IOException {
        if(present())throw new IOException("Validation mouse already exists");
        pipes=automation.executeShellCommandRw("uinput -");
        writer=new ParcelFileDescriptor.AutoCloseOutputStream(pipes[1]);
        try{
            send("{\"id\":1,\"command\":\"register\",\"name\":\""+NAME+"\",\"vid\":6353,\"pid\":6550,\"bus\":\"usb\",\"configuration\":[{\"type\":100,\"data\":[1,2]},{\"type\":101,\"data\":[272]},{\"type\":102,\"data\":[0,1]}]}");
            long end=SystemClock.uptimeMillis()+10000;
            while(!present()&&SystemClock.uptimeMillis()<end)SystemClock.sleep(50);
            if(!present())throw new IOException("Kernel mouse registration failed");
        }catch(IOException|RuntimeException failure){close();throw failure;}
    }
    static boolean present(){for(int id:InputDevice.getDeviceIds()){InputDevice d=InputDevice.getDevice(id);if(d!=null&&NAME.equals(d.getName())&&d.supportsSource(InputDevice.SOURCE_MOUSE))return true;}return false;}
    private void send(String value)throws IOException{writer.write((value+"\n").getBytes(StandardCharsets.UTF_8));writer.flush();}
    void move(int dx,int dy)throws IOException{
        if(Math.abs(dx)>2000||Math.abs(dy)>2000)throw new IllegalArgumentException("Unbounded mouse move");
        send("{\"id\":1,\"command\":\"updateTimeBase\"}");
        send("{\"id\":1,\"command\":\"inject\",\"events\":[2,0,"+dx+",2,1,"+dy+",0,0,0]}");
    }
    @Override public void close()throws IOException{
        writer.close();for(int i=0;i<pipes.length;i++)if(i!=1)pipes[i].close();
        long end=SystemClock.uptimeMillis()+5000;while(present()&&SystemClock.uptimeMillis()<end)SystemClock.sleep(50);
        if(present())throw new IOException("Validation mouse remained registered");
    }
}
