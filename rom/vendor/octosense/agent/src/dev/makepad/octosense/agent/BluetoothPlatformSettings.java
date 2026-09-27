package dev.makepad.octosense.agent;

import android.bluetooth.BluetoothDevice;
import android.bluetooth.BluetoothStatusCodes;
import dev.makepad.octosense.bluetooth.BluetoothSettingsBackend;
import dev.makepad.octosense.bluetooth.BluetoothSettingsContract;

/** Hidden framework calls linked only into the signature-gated ROM helper. */
final class BluetoothPlatformSettings implements BluetoothSettingsBackend.Platform {
    @Override public String connection(BluetoothDevice device) {return device.isConnected()?"connected":"disconnected";}
    @Override public String sharing(BluetoothDevice device,String kind) {
        BluetoothSettingsContract.sharingKind(kind);
        int value=kind.equals("phonebook")?device.getPhonebookAccessPermission():device.getMessageAccessPermission();
        switch(value) {
            case BluetoothDevice.ACCESS_UNKNOWN:return "ask";
            case BluetoothDevice.ACCESS_ALLOWED:return "allow";
            case BluetoothDevice.ACCESS_REJECTED:return "deny";
            default:return null;
        }
    }
    @Override public boolean action(BluetoothDevice device,BluetoothSettingsContract.Action action) {
        switch(action) {
            case CANCEL_PAIR:return device.cancelBondProcess();
            case FORGET:return device.removeBond();
            case CONNECT:return device.connect()==BluetoothStatusCodes.SUCCESS;
            case DISCONNECT:return device.disconnect()==BluetoothStatusCodes.SUCCESS;
            default:return false;
        }
    }
    @Override public boolean sharing(BluetoothDevice device,String kind,String value) {
        BluetoothSettingsContract.sharingKind(kind);BluetoothSettingsContract.sharingValue(value);
        int permission=value.equals("ask")?BluetoothDevice.ACCESS_UNKNOWN:value.equals("allow")?BluetoothDevice.ACCESS_ALLOWED:BluetoothDevice.ACCESS_REJECTED;
        return kind.equals("phonebook")?device.setPhonebookAccessPermission(permission):device.setMessageAccessPermission(permission);
    }
}
